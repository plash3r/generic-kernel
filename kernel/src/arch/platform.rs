use bootloader_api::BootInfo;

pub fn init(info: &BootInfo) {
    let rsdp = info
        .rsdp_addr
        .into_option()
        .expect("ACPI RSDP required for Generic interrupt platform");
    let physical_offset =
        crate::mm::physical_memory_offset().expect("physical direct map required for ACPI");

    let apic_info = crate::arch::acpi::discover_apic(rsdp, physical_offset)
        .expect("ACPI MADT discovery failed");
    crate::arch::apic::init(&apic_info).expect("APIC/IOAPIC initialization failed");
    crate::arch::timer::init();
    let _ = crate::arch::ps2::init();

    x86_64::instructions::interrupts::enable();
    crate::arch::timer::wait_ticks(3);
    crate::log!(
        "[ok] interrupt event loop + PIT timer {} Hz ({} ticks)\n",
        crate::arch::timer::HZ,
        crate::arch::timer::ticks()
    );
}
