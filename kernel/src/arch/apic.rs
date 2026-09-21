use crate::{arch::acpi::ApicInfo, mm};
use core::{
    arch::asm,
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering},
};

const IA32_APIC_BASE: u32 = 0x1b;
const APIC_ENABLE: u64 = 1 << 11;
const X2APIC_ENABLE: u64 = 1 << 10;
const APIC_BASE_MASK: u64 = 0x000f_ffff_ffff_f000;

const LAPIC_ID: usize = 0x020;
const LAPIC_TPR: usize = 0x080;
const LAPIC_EOI: usize = 0x0b0;
const LAPIC_SVR: usize = 0x0f0;
const LAPIC_LVT_TIMER: usize = 0x320;
const LAPIC_LVT_LINT0: usize = 0x350;
const LAPIC_LVT_LINT1: usize = 0x360;
const LAPIC_LVT_ERROR: usize = 0x370;
const LVT_MASKED: u32 = 1 << 16;

const IOREGSEL: usize = 0x00;
const IOWIN: usize = 0x10;
const IOAPIC_VERSION: u8 = 0x01;
const IOAPIC_REDTBL: u8 = 0x10;
const REDIRECT_MASKED: u32 = 1 << 16;
const REDIRECT_ACTIVE_LOW: u32 = 1 << 13;
const REDIRECT_LEVEL: u32 = 1 << 15;

const SPURIOUS_VECTOR: u8 = 0xff;

static LAPIC_VIRTUAL: AtomicU64 = AtomicU64::new(0);
static LAPIC_ID_VALUE: AtomicU32 = AtomicU32::new(0);
static IOAPIC_COUNT: AtomicUsize = AtomicUsize::new(0);
static INITIALIZED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct IoApic {
    virtual_address: u64,
    gsi_base: u32,
    redirections: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Diagnostics {
    pub initialized: bool,
    pub local_apic_id: u32,
    pub io_apic_count: usize,
}

pub fn init(info: &ApicInfo) -> Result<(), &'static str> {
    if INITIALIZED.load(Ordering::SeqCst) {
        return Err("APIC initialized twice");
    }
    if !cpu_has_apic() {
        return Err("CPU does not report local APIC support");
    }

    mask_legacy_pic();

    let lapic_virtual =
        mm::map_mmio(info.local_apic_address, 4096).ok_or("failed to map local APIC MMIO")?;
    enable_local_apic(info.local_apic_address);
    LAPIC_VIRTUAL.store(lapic_virtual, Ordering::SeqCst);

    lapic_write(LAPIC_TPR, 0);
    lapic_write(LAPIC_LVT_TIMER, LVT_MASKED);
    lapic_write(LAPIC_LVT_LINT0, LVT_MASKED);
    lapic_write(LAPIC_LVT_LINT1, LVT_MASKED);
    lapic_write(LAPIC_LVT_ERROR, LVT_MASKED);
    lapic_write(LAPIC_SVR, (1 << 8) | SPURIOUS_VECTOR as u32);

    let local_id = lapic_read(LAPIC_ID) >> 24;
    LAPIC_ID_VALUE.store(local_id, Ordering::SeqCst);

    let mut io_apics: [Option<IoApic>; 4] = [None; 4];
    for (index, descriptor) in info.io_apics[..info.io_apic_count].iter().enumerate() {
        let virtual_address =
            mm::map_mmio(descriptor.address as u64, 4096).ok_or("failed to map IOAPIC MMIO")?;
        let mut ioapic = IoApic {
            virtual_address,
            gsi_base: descriptor.gsi_base,
            redirections: 0,
        };
        let version = ioapic.read(IOAPIC_VERSION);
        ioapic.redirections = ((version >> 16) & 0xff) + 1;
        ioapic.mask_all();
        io_apics[index] = Some(ioapic);
    }

    route_isa_irq(
        &io_apics,
        info,
        0,
        crate::arch::interrupts::TIMER_VECTOR,
        local_id,
    )?;
    route_isa_irq(
        &io_apics,
        info,
        1,
        crate::arch::interrupts::KEYBOARD_VECTOR,
        local_id,
    )?;
    route_isa_irq(
        &io_apics,
        info,
        12,
        crate::arch::interrupts::MOUSE_VECTOR,
        local_id,
    )?;

    IOAPIC_COUNT.store(info.io_apic_count, Ordering::SeqCst);
    INITIALIZED.store(true, Ordering::SeqCst);
    crate::log!(
        "[ok] xAPIC id={} + {} IOAPIC(s), IRQ0/1/12 routed\n",
        local_id,
        info.io_apic_count
    );
    Ok(())
}

pub fn eoi() {
    if LAPIC_VIRTUAL.load(Ordering::Relaxed) != 0 {
        lapic_write(LAPIC_EOI, 0);
    }
}

pub fn diagnostics() -> Diagnostics {
    Diagnostics {
        initialized: INITIALIZED.load(Ordering::SeqCst),
        local_apic_id: LAPIC_ID_VALUE.load(Ordering::SeqCst),
        io_apic_count: IOAPIC_COUNT.load(Ordering::SeqCst),
    }
}

fn route_isa_irq(
    io_apics: &[Option<IoApic>; 4],
    info: &ApicInfo,
    irq: u8,
    vector: u8,
    destination: u32,
) -> Result<(), &'static str> {
    let override_entry = info.overrides[irq as usize];
    let (gsi, flags) = match override_entry {
        Some(entry) => (entry.gsi, entry.flags),
        None => (irq as u32, 0),
    };

    let ioapic = io_apics
        .iter()
        .flatten()
        .find(|ioapic| gsi >= ioapic.gsi_base && gsi < ioapic.gsi_base + ioapic.redirections)
        .ok_or("no IOAPIC owns requested GSI")?;

    let mut low = vector as u32;
    let polarity = flags & 0b11;
    let trigger = (flags >> 2) & 0b11;

    if polarity == 0b11 {
        low |= REDIRECT_ACTIVE_LOW;
    } else if polarity != 0 && polarity != 0b01 {
        return Err("unsupported ACPI interrupt polarity");
    }

    if trigger == 0b11 {
        low |= REDIRECT_LEVEL;
    } else if trigger != 0 && trigger != 0b01 {
        return Err("unsupported ACPI interrupt trigger mode");
    }

    ioapic.write_redirection(gsi - ioapic.gsi_base, low, destination << 24);
    Ok(())
}

impl IoApic {
    fn read(&self, register: u8) -> u32 {
        // SAFETY: virtual_address is a dedicated uncached mapping of one IOAPIC page.
        unsafe {
            core::ptr::write_volatile(
                (self.virtual_address as usize + IOREGSEL) as *mut u32,
                register as u32,
            );
            core::ptr::read_volatile((self.virtual_address as usize + IOWIN) as *const u32)
        }
    }

    fn write(&self, register: u8, value: u32) {
        // SAFETY: virtual_address is a dedicated uncached mapping of one IOAPIC page.
        unsafe {
            core::ptr::write_volatile(
                (self.virtual_address as usize + IOREGSEL) as *mut u32,
                register as u32,
            );
            core::ptr::write_volatile((self.virtual_address as usize + IOWIN) as *mut u32, value);
        }
    }

    fn mask_all(&mut self) {
        for index in 0..self.redirections {
            self.write_redirection(index, REDIRECT_MASKED, 0);
        }
    }

    fn write_redirection(&self, index: u32, low: u32, high: u32) {
        let register = IOAPIC_REDTBL as u32 + index * 2;
        self.write((register + 1) as u8, high);
        self.write(register as u8, low);
    }
}

fn lapic_read(offset: usize) -> u32 {
    let base = LAPIC_VIRTUAL.load(Ordering::Relaxed) as usize;
    // SAFETY: init installs a dedicated uncached local APIC MMIO mapping.
    unsafe { core::ptr::read_volatile((base + offset) as *const u32) }
}

fn lapic_write(offset: usize, value: u32) {
    let base = LAPIC_VIRTUAL.load(Ordering::Relaxed) as usize;
    // SAFETY: init installs a dedicated uncached local APIC MMIO mapping.
    unsafe { core::ptr::write_volatile((base + offset) as *mut u32, value) }
}

fn enable_local_apic(physical_base: u64) {
    let mut value = read_msr(IA32_APIC_BASE);
    value &= !APIC_BASE_MASK;
    value &= !X2APIC_ENABLE;
    value |= physical_base & APIC_BASE_MASK;
    value |= APIC_ENABLE;
    write_msr(IA32_APIC_BASE, value);
}

fn cpu_has_apic() -> bool {
    // SAFETY: CPUID leaf 1 is universally available on x86_64.
    let leaf = unsafe { core::arch::x86_64::__cpuid(1) };
    leaf.edx & (1 << 9) != 0
}

fn mask_legacy_pic() {
    use x86_64::instructions::port::Port;
    // SAFETY: 0x21 and 0xa1 are the legacy PIC mask registers.
    unsafe {
        Port::<u8>::new(0x21).write(0xff);
        Port::<u8>::new(0xa1).write(0xff);
    }
}

fn read_msr(register: u32) -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: caller uses an architectural MSR supported on APIC-capable x86_64.
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") register,
            out("eax") low,
            out("edx") high,
            options(nostack, nomem)
        );
    }
    low as u64 | ((high as u64) << 32)
}

fn write_msr(register: u32, value: u64) {
    // SAFETY: caller uses an architectural MSR supported on APIC-capable x86_64.
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") register,
            in("eax") value as u32,
            in("edx") (value >> 32) as u32,
            options(nostack, nomem)
        );
    }
}
