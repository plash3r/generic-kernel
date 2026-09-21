#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

mod arch;
mod mm;
mod recontrol;
mod shell;
mod vfs;

use bootloader_api::{config::Mapping, BootInfo, BootloaderConfig};

static CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config.kernel_stack_size = 128 * 1024;
    config
};
bootloader_api::entry_point!(kernel_main, config = &CONFIG);

fn kernel_main(info: &'static mut BootInfo) -> ! {
    x86_64::instructions::interrupts::disable();
    arch::serial::init();
    log!("GENERIC: boot\n");

    arch::interrupts::init();
    log!("[ok] GDT / TSS / IDT\n");

    // Execute a small function compiled from Recontrol and linked directly into
    // this no_std kernel image. This validates the compiler/kernel ABI before
    // relying on Recontrol for larger system components.
    recontrol::verify();

    // Keep generic allocation policy separate from x86_64 page-table mechanics:
    // mm owns the PMM/heap policy, arch::memory owns active page-table access.
    mm::init(info);
    vfs::init();

    x86_64::instructions::interrupts::int3();
    log!("[ok] breakpoint returned\n");
    log!("GENERIC: READY\n");

    if let Some(framebuffer) = info.framebuffer.as_mut() {
        let mut console = arch::framebuffer::Console::new(framebuffer);
        console.set_accent_color();
        use core::fmt::Write;
        let width = console.width();
        let height = console.height();
        let _ = writeln!(console, "GENERIC framebuffer {width}x{height} READY");
        console.set_default_color();
        log!("[ok] framebuffer console {}x{}\n", width, height);

        #[cfg(feature = "smoke")]
        arch::exit(true);

        #[cfg(not(feature = "smoke"))]
        shell::run(console);
    }

    log!("[warn] no framebuffer supplied by bootloader\n");

    #[cfg(feature = "smoke")]
    arch::exit(true);

    #[cfg(not(feature = "smoke"))]
    arch::halt()
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    log!("GENERIC: PANIC: {}\n", info);
    #[cfg(feature = "smoke")]
    arch::exit(false);
    #[cfg(not(feature = "smoke"))]
    arch::halt()
}

#[alloc_error_handler]
fn alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("kernel allocation failed: {layout:?}")
}
