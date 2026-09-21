use spin::Lazy;
use x86_64::{
    instructions::{
        segmentation::{Segment, CS, DS, ES, SS},
        tables::load_tss,
    },
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode},
        tss::TaskStateSegment,
    },
    VirtAddr,
};

pub const TIMER_VECTOR: u8 = 0x40;
pub const KEYBOARD_VECTOR: u8 = 0x41;
pub const MOUSE_VECTOR: u8 = 0x42;
const SPURIOUS_VECTOR: u8 = 0xff;

const DOUBLE_FAULT_IST: u16 = 0;

#[repr(align(16))]
struct Stack {
    _bytes: [u8; 32 * 1024],
}

static mut FAULT_STACK: Stack = Stack {
    _bytes: [0; 32 * 1024],
};

static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    let start = core::ptr::addr_of_mut!(FAULT_STACK) as u64;
    tss.interrupt_stack_table[DOUBLE_FAULT_IST as usize] =
        VirtAddr::new(start + core::mem::size_of::<Stack>() as u64);
    tss
});

static GDT: Lazy<(
    GlobalDescriptorTable,
    SegmentSelector,
    SegmentSelector,
    SegmentSelector,
)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let code = gdt.append(Descriptor::kernel_code_segment());
    let data = gdt.append(Descriptor::kernel_data_segment());
    let tss = gdt.append(Descriptor::tss_segment(&TSS));
    (gdt, code, data, tss)
});

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.breakpoint.set_handler_fn(breakpoint);
    idt.invalid_opcode.set_handler_fn(invalid_opcode);
    idt.general_protection_fault
        .set_handler_fn(general_protection);
    idt.page_fault.set_handler_fn(page_fault);
    // SAFETY: TSS has a dedicated static stack at this IST index, loaded before IDT.
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault)
            .set_stack_index(DOUBLE_FAULT_IST);
    }

    idt[TIMER_VECTOR].set_handler_fn(timer_interrupt);
    idt[KEYBOARD_VECTOR].set_handler_fn(keyboard_interrupt);
    idt[MOUSE_VECTOR].set_handler_fn(mouse_interrupt);
    idt[SPURIOUS_VECTOR].set_handler_fn(spurious_interrupt);
    idt
});

pub fn init() {
    GDT.0.load();
    // SAFETY: selectors reference the newly loaded static GDT.
    unsafe {
        CS::set_reg(GDT.1);
        DS::set_reg(GDT.2);
        ES::set_reg(GDT.2);
        SS::set_reg(GDT.2);
        load_tss(GDT.3);
    }
    IDT.load();
}

extern "x86-interrupt" fn timer_interrupt(_frame: InterruptStackFrame) {
    crate::arch::timer::interrupt();
    crate::arch::apic::eoi();
}

extern "x86-interrupt" fn keyboard_interrupt(_frame: InterruptStackFrame) {
    crate::arch::keyboard::interrupt();
    crate::arch::apic::eoi();
}

extern "x86-interrupt" fn mouse_interrupt(_frame: InterruptStackFrame) {
    if let Some(byte) = crate::arch::ps2::read_interrupt_data(true) {
        crate::arch::mouse::interrupt_byte(byte);
    }
    crate::arch::apic::eoi();
}

extern "x86-interrupt" fn spurious_interrupt(_frame: InterruptStackFrame) {}

extern "x86-interrupt" fn breakpoint(frame: InterruptStackFrame) {
    crate::log!(
        "[exception] breakpoint at {:?}\n",
        frame.instruction_pointer
    );
}

extern "x86-interrupt" fn invalid_opcode(frame: InterruptStackFrame) {
    panic!("invalid opcode: {frame:?}");
}

extern "x86-interrupt" fn general_protection(frame: InterruptStackFrame, code: u64) {
    panic!("GP {code:#x}: {frame:?}");
}

extern "x86-interrupt" fn page_fault(frame: InterruptStackFrame, code: PageFaultErrorCode) {
    panic!(
        "page fault {:?}, {:?}: {:?}",
        x86_64::registers::control::Cr2::read(),
        code,
        frame
    );
}

extern "x86-interrupt" fn double_fault(frame: InterruptStackFrame, code: u64) -> ! {
    panic!("double fault {code}: {frame:?}");
}
