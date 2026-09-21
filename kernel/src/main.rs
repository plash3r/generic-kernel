#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

mod arch;
mod initramfs;
mod mm;
mod process;
mod recontrol;
mod shell;
mod task;
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
    log!("GENERIC: boot
");

    arch::interrupts::init();
    log!("[ok] GDT / TSS / IDT
");

    // Execute a small function compiled from Recontrol and linked directly into
    // this no_std kernel image. This validates the compiler/kernel ABI before
    // relying on Recontrol for larger system components.
    recontrol::verify();

    // Keep generic allocation policy separate from x86_64 page-table mechanics:
    // mm owns the PMM/heap policy, arch::memory owns active page-table access.
    mm::init(info);

    let block_device = match arch::virtio_blk::VirtioBlock::probe() {
        Ok(Some(device)) => Some(alloc::boxed::Box::new(device)
            as alloc::boxed::Box<dyn kernel_core::block::BlockDevice>),
        Ok(None) => None,
        Err(error) => {
            log!("[warn] virtio-blk initialization failed: {}
", error);
            None
        }
    };
    vfs::init(block_device);

    // Discover ACPI interrupt topology, install xAPIC/IOAPIC routing, initialize
    // PS/2 event queues and start the first Generic system timer.
    arch::platform::init(info);
    task::init();

    #[cfg(feature = "smoke")]
    task::smoke_test();

    if let Some(framebuffer) = info.framebuffer.as_mut() {
        let framebuffer_info = framebuffer.info();
        arch::display::init(framebuffer);
        log!(
            "[ok] display ABI bridge {}x{} XRGB8888 userspace present
",
            framebuffer_info.width,
            framebuffer_info.height
        );
    }

    process::init_probe();

    x86_64::instructions::interrupts::int3();
    log!("[ok] breakpoint returned
");
    log!("GENERIC: READY
");

    #[cfg(not(feature = "smoke"))]
    if arch::display::is_ready() {
        match process::launch_graphical_shell() {
            Ok(exit) => log!("[warn] userspace graphical shell exited with {}
", exit),
            Err(error) => log!("[warn] graphical shell unavailable: {}
", error),
        }
    }

    if let Some(framebuffer) = info.framebuffer.as_mut() {
        let framebuffer_info = framebuffer.info();
        log!(
            "[ok] framebuffer {}x{} ready for console
",
            framebuffer_info.width,
            framebuffer_info.height
        );

        #[cfg(feature = "smoke")]
        {
            let mut console = arch::framebuffer::Console::new(framebuffer);
            for byte in b"GENERIC framebuffer console smoke
" {
                console.write_byte(*byte);
            }
            log!("[ok] framebuffer console smoke
");
            arch::exit(true);
        }

        #[cfg(not(feature = "smoke"))]
        {
            let console = arch::framebuffer::Console::new(framebuffer);
            shell::run(console);
        }
    }

    log!("[warn] no framebuffer supplied by bootloader
");

    #[cfg(feature = "smoke")]
    arch::exit(true);

    #[cfg(not(feature = "smoke"))]
    arch::halt()
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    log!("GENERIC: PANIC: {}
", info);
    #[cfg(feature = "smoke")]
    arch::exit(false);
    #[cfg(not(feature = "smoke"))]
    arch::halt()
}

#[alloc_error_handler]
fn alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("kernel allocation failed: {layout:?}")
}
