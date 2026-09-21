#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![deny(unsafe_op_in_unsafe_fn)]

mod arch;
use bootloader_api::{config::Mapping, info::MemoryRegionKind, BootInfo, BootloaderConfig};
use kernel_core::{FrameAllocator, Region, PAGE_SIZE};

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
    let offset = info
        .physical_memory_offset
        .into_option()
        .expect("physical mapping required");
    let mut usable = [Region::default(); 256];
    let mut count = 0;
    for region in info.memory_regions.iter() {
        if region.kind == MemoryRegionKind::Usable {
            assert!(count < usable.len(), "too many usable memory regions");
            usable[count] = Region {
                start: region.start,
                end: region.end,
            };
            count += 1;
        }
    }
    usable[..count].sort_unstable_by_key(|r| r.start);
    let mut frames = FrameAllocator::new(&usable[..count]).expect("invalid physical memory map");
    let first = frames.allocate().expect("no usable RAM");
    let second = frames.allocate().expect("insufficient usable RAM");
    assert_ne!(first, second);
    for physical in [first, second] {
        let virtual_address = offset
            .checked_add(physical)
            .expect("mapping address overflow");
        let ptr = x86_64::VirtAddr::new(virtual_address).as_mut_ptr::<u8>();
        // SAFETY: the bootloader maps all physical memory at offset. These unique,
        // usable frames exclude loader/kernel/firmware allocations. No aliases exist.
        unsafe {
            core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
            core::ptr::write_volatile(ptr, 0xa5);
            assert_eq!(core::ptr::read_volatile(ptr), 0xa5);
            core::ptr::write_volatile(ptr, 0);
        }
    }
    log!(
        "[ok] physical frames {:#x}, {:#x} ({} usable regions)\n",
        first,
        second,
        count
    );
    x86_64::instructions::interrupts::int3();
    log!("[ok] breakpoint returned\n");
    log!("GENERIC: READY\n");
    // Keep this allocator's consumed-frame state when adding the next boot stages.
    // Hardware interrupts remain masked until IRQ controllers and handlers exist.
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
