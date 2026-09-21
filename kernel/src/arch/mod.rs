pub mod acpi;
pub mod apic;
pub mod display;
pub mod framebuffer;
pub mod input;
pub mod interrupts;
pub mod keyboard;
pub mod memory;
pub mod mouse;
pub mod pci;
pub mod platform;
pub mod ps2;
pub mod serial;
pub mod timer;
pub mod user;
pub mod virtio_blk;

pub fn halt() -> ! {
    x86_64::instructions::interrupts::disable();
    loop {
        x86_64::instructions::hlt();
    }
}

pub fn reboot() -> ! {
    x86_64::instructions::interrupts::disable();

    // Ask the legacy i8042 controller to pulse the CPU reset line. VirtualBox
    // and QEMU both expose this compatibility path for a PS/2 machine.
    for _ in 0..100_000 {
        // SAFETY: port 0x64 is the standard i8042 status/command register.
        let status = unsafe { x86_64::instructions::port::Port::<u8>::new(0x64).read() };
        if status & 0x02 == 0 {
            // SAFETY: 0xfe is the i8042 CPU reset command.
            unsafe {
                x86_64::instructions::port::Port::<u8>::new(0x64).write(0xfe);
            }
            break;
        }
        core::hint::spin_loop();
    }

    halt()
}

#[cfg(feature = "smoke")]
pub fn exit(success: bool) -> ! {
    // SAFETY: QEMU's isa-debug-exit device is explicitly attached at this port.
    unsafe {
        x86_64::instructions::port::Port::<u32>::new(0xf4).write(if success { 0x10 } else { 0x11 });
    }
    halt()
}
