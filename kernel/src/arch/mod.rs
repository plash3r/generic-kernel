pub mod interrupts;
pub mod serial;

pub fn halt() -> ! {
    x86_64::instructions::interrupts::disable();
    loop {
        x86_64::instructions::hlt();
    }
}

#[cfg(feature = "smoke")]
pub fn exit(success: bool) -> ! {
    // SAFETY: QEMU's isa-debug-exit device is explicitly attached at this port.
    unsafe {
        x86_64::instructions::port::Port::<u32>::new(0xf4).write(if success { 0x10 } else { 0x11 });
    }
    halt()
}
