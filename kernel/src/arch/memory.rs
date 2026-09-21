use kernel_core::{PhysicalMemory, PAGE_SIZE};
use x86_64::{
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator as X86FrameAllocator, Mapper, OffsetPageTable, Page, PageTable,
        PageTableFlags, PhysFrame, Size4KiB, Translate,
    },
    PhysAddr, VirtAddr,
};

pub struct HeapMapping {
    pub mapped_pages: u64,
    pub page_table_and_heap_frames: u64,
}

struct PmmFrameAllocator<'a, const N: usize> {
    pmm: &'a mut PhysicalMemory<N>,
    allocated: u64,
}

impl<'a, const N: usize> PmmFrameAllocator<'a, N> {
    fn new(pmm: &'a mut PhysicalMemory<N>) -> Self {
        Self { pmm, allocated: 0 }
    }

    fn allocate_physical(&mut self) -> Option<u64> {
        let address = self.pmm.allocate_frame().ok().flatten()?;
        self.allocated += 1;
        Some(address)
    }
}

// SAFETY: PhysicalMemory returns unique page-aligned frames from firmware
// regions classified as usable. Allocated frames are removed from its free set
// until explicitly returned, so page-table frames cannot alias.
unsafe impl<const N: usize> X86FrameAllocator<Size4KiB> for PmmFrameAllocator<'_, N> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.allocate_physical()
            .map(|address| PhysFrame::containing_address(PhysAddr::new(address)))
    }
}

/// Map a writable, non-executable kernel heap while deliberately leaving one
/// unmapped guard page on each side.
pub fn map_heap<const N: usize>(
    physical_memory_offset: u64,
    pmm: &mut PhysicalMemory<N>,
    heap_start: u64,
    heap_size: u64,
) -> HeapMapping {
    assert_eq!(heap_start % PAGE_SIZE, 0, "heap start must be page aligned");
    assert_eq!(heap_size % PAGE_SIZE, 0, "heap size must be page aligned");
    assert!(heap_size >= PAGE_SIZE, "heap must contain at least one page");

    let heap_end = heap_start
        .checked_add(heap_size)
        .expect("heap virtual-address overflow");
    let lower_guard = heap_start
        .checked_sub(PAGE_SIZE)
        .expect("heap lower guard underflow");
    let upper_guard = heap_end;

    let physical_offset = VirtAddr::new(physical_memory_offset);
    // SAFETY: the bootloader supplied a direct physical-memory mapping and the
    // active CR3 frame points at the currently used level-4 table.
    let mut mapper = unsafe { current_offset_page_table(physical_offset) };

    for address in [lower_guard, upper_guard] {
        assert!(
            mapper.translate_addr(VirtAddr::new(address)).is_none(),
            "kernel heap guard page is already mapped"
        );
    }

    let start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(heap_start));
    let end_page =
        Page::<Size4KiB>::containing_address(VirtAddr::new(heap_end - PAGE_SIZE));

    for page in Page::range_inclusive(start_page, end_page) {
        assert!(
            mapper.translate_addr(page.start_address()).is_none(),
            "kernel heap virtual range is already mapped"
        );
    }

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
    let mut allocator = PmmFrameAllocator::new(pmm);
    let mapped_pages = heap_size / PAGE_SIZE;

    for page in Page::range_inclusive(start_page, end_page) {
        let physical = allocator
            .allocate_physical()
            .expect("out of physical memory while mapping kernel heap");
        zero_physical_frame(physical_offset, physical);
        let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(physical));

        // SAFETY: page is verified unmapped, frame is uniquely allocated from
        // the PMM, and the flags intentionally make heap memory RW + NX.
        unsafe {
            mapper
                .map_to(page, frame, flags, &mut allocator)
                .expect("failed to create kernel heap mapping")
                .flush();
        }
    }

    assert!(
        mapper.translate_addr(VirtAddr::new(lower_guard)).is_none()
            && mapper.translate_addr(VirtAddr::new(upper_guard)).is_none(),
        "kernel heap guard pages became mapped"
    );

    HeapMapping {
        mapped_pages,
        page_table_and_heap_frames: allocator.allocated,
    }
}

fn zero_physical_frame(physical_memory_offset: u64, physical: u64) {
    let virtual_address = physical_memory_offset
        .checked_add(physical)
        .expect("physical direct-map address overflow");
    let ptr = VirtAddr::new(virtual_address).as_mut_ptr::<u8>();
    // SAFETY: the bootloader maps physical RAM at physical_memory_offset and
    // this frame is uniquely owned by the PMM allocation performed above.
    unsafe {
        core::ptr::write_bytes(ptr, 0, PAGE_SIZE as usize);
    }
}

unsafe fn current_offset_page_table(
    physical_memory_offset: VirtAddr,
) -> OffsetPageTable<'static> {
    let (level_4_frame, _) = Cr3::read();
    let physical = level_4_frame.start_address();
    let virtual_address = physical_memory_offset + physical.as_u64();
    let page_table_ptr: *mut PageTable = virtual_address.as_mut_ptr();

    // SAFETY: caller guarantees the physical direct map is valid and no second
    // mutable PageTable reference is created while this mapper is alive.
    let level_4_table = unsafe { &mut *page_table_ptr };
    // SAFETY: physical_memory_offset maps every physical frame at a constant
    // virtual offset, which is the contract required by OffsetPageTable.
    unsafe { OffsetPageTable::new(level_4_table, physical_memory_offset) }
}
