pub mod heap;

use bootloader_api::{info::MemoryRegionKind, BootInfo};
use kernel_core::{PhysicalMemory, Region, PAGE_SIZE};
use spin::Mutex;
use x86_64::VirtAddr;

const MAX_BOOT_REGIONS: usize = 256;
const PMM_REGION_CAPACITY: usize = 512;

pub const HEAP_START: u64 = 0x0000_4444_0000_0000;
pub const HEAP_SIZE: u64 = 2 * 1024 * 1024;

static PHYSICAL_MEMORY: Mutex<Option<PhysicalMemory<PMM_REGION_CAPACITY>>> = Mutex::new(None);

#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryStats {
    pub physical_total: u64,
    pub physical_free: u64,
    pub managed_regions: usize,
    pub heap_total: usize,
    pub heap_free: usize,
}

pub fn init(info: &BootInfo) {
    let physical_memory_offset = info
        .physical_memory_offset
        .into_option()
        .expect("physical mapping required");

    let mut regions = [Region::default(); MAX_BOOT_REGIONS];
    let mut count = 0usize;
    for region in info.memory_regions.iter() {
        if region.kind != MemoryRegionKind::Usable {
            continue;
        }
        assert!(
            count < regions.len(),
            "too many usable physical-memory regions"
        );
        regions[count] = Region {
            start: region.start,
            end: region.end,
        };
        count += 1;
    }
    regions[..count].sort_unstable_by_key(|region| region.start);

    let mut pmm = PhysicalMemory::<PMM_REGION_CAPACITY>::new(&regions[..count])
        .expect("invalid physical-memory map");
    let initial_total = pmm.total_bytes();

    verify_physical_memory(physical_memory_offset, &mut pmm);

    let before_heap = pmm.free_bytes();
    let mapping =
        crate::arch::memory::map_heap(physical_memory_offset, &mut pmm, HEAP_START, HEAP_SIZE);
    let consumed = before_heap - pmm.free_bytes();
    assert_eq!(
        consumed / PAGE_SIZE,
        mapping.page_table_and_heap_frames,
        "PMM accounting disagrees with heap mapper"
    );

    heap::KERNEL_ALLOCATOR.init(HEAP_START as usize, HEAP_SIZE as usize);
    heap::KERNEL_ALLOCATOR.verify();

    crate::log!(
        "[ok] PMM {} MiB managed in {} regions, {} MiB free\n",
        initial_total / (1024 * 1024),
        pmm.managed_region_count(),
        pmm.free_bytes() / (1024 * 1024)
    );
    crate::log!(
        "[ok] kernel heap {} KiB @ {:#x}, {} pages, RW+NX, guard pages\n",
        HEAP_SIZE / 1024,
        HEAP_START,
        mapping.mapped_pages
    );

    let mut global = PHYSICAL_MEMORY.lock();
    assert!(global.is_none(), "physical memory initialized twice");
    *global = Some(pmm);
}

pub fn stats() -> MemoryStats {
    let physical = PHYSICAL_MEMORY.lock();
    let Some(pmm) = physical.as_ref() else {
        return MemoryStats::default();
    };

    MemoryStats {
        physical_total: pmm.total_bytes(),
        physical_free: pmm.free_bytes(),
        managed_regions: pmm.managed_region_count(),
        heap_total: heap::KERNEL_ALLOCATOR.total_bytes(),
        heap_free: heap::KERNEL_ALLOCATOR.free_bytes(),
    }
}

fn verify_physical_memory(
    physical_memory_offset: u64,
    pmm: &mut PhysicalMemory<PMM_REGION_CAPACITY>,
) {
    let first = pmm
        .allocate_frame()
        .expect("PMM allocation error")
        .expect("no usable RAM");
    let second = pmm
        .allocate_frame()
        .expect("PMM allocation error")
        .expect("insufficient usable RAM");
    assert_ne!(first, second);

    for physical in [first, second] {
        let virtual_address = physical_memory_offset
            .checked_add(physical)
            .expect("physical direct-map address overflow");
        let ptr = VirtAddr::new(virtual_address).as_mut_ptr::<u8>();

        // SAFETY: the bootloader direct-maps physical memory and the PMM has
        // removed this unique frame from the free set for the duration of test.
        unsafe {
            core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
            core::ptr::write_volatile(ptr, 0xa5);
            assert_eq!(core::ptr::read_volatile(ptr), 0xa5);
            core::ptr::write_volatile(ptr, 0);
        }
    }

    pmm.free_pages(second, 1)
        .expect("failed to return PMM smoke frame");
    pmm.free_pages(first, 1)
        .expect("failed to return PMM smoke frame");

    crate::log!(
        "[ok] physical frames {:#x}, {:#x}, allocate/free/coalesce\n",
        first,
        second
    );
}
