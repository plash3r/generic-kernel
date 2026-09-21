use kernel_core::page_tables::{clone_root, TableMemory};
use kernel_core::{PhysicalMemory, PAGE_SIZE};
use x86_64::{
    registers::control::{Cr0, Cr0Flags, Cr3, Cr4, Cr4Flags},
    structures::paging::{
        FrameAllocator as X86FrameAllocator, Mapper, OffsetPageTable, Page, PageTable,
        PageTableFlags, PhysFrame, Size4KiB, Translate,
    },
    PhysAddr, VirtAddr,
};

struct BootTables<'a, const N: usize> {
    offset: u64,
    pmm: &'a mut PhysicalMemory<N>,
}

impl<const N: usize> BootTables<'_, N> {
    fn pointer(&self, physical: u64, index: usize) -> *mut u64 {
        assert!(index < 512);
        let address = self
            .offset
            .checked_add(physical)
            .expect("table direct-map overflow");
        // SAFETY: index is within a single 4 KiB table. The boot contract maps
        // all page-table frames at offset. Only this bootstrap CPU is running.
        unsafe { VirtAddr::new(address).as_mut_ptr::<u64>().add(index) }
    }
}

impl<const N: usize> TableMemory for BootTables<'_, N> {
    fn allocate(&mut self) -> Option<u64> {
        let frame = self.pmm.allocate_frame().ok().flatten()?;
        zero_physical_frame(self.offset, frame);
        Some(frame)
    }
    fn release(&mut self, frame: u64) {
        self.pmm
            .free_pages(frame, 1)
            .expect("table rollback failed");
    }
    fn read(&self, frame: u64, index: usize) -> u64 {
        // SAFETY: the source is the active, bootloader-provided table tree.
        // Volatile access does not create references aliasing CPU A/D writes.
        unsafe { self.pointer(frame, index).read_volatile() }
    }
    fn write(&mut self, frame: u64, index: usize, entry: u64) {
        // SAFETY: clone_root writes only exclusively allocated, inactive tables.
        unsafe { self.pointer(frame, index).write_volatile(entry) }
    }
}

/// Bootstrap-only transition; every non-leaf table is now owned by the PMM.
/// Leaf mappings and permissions remain unchanged, including framebuffer,
/// kernel stack, descriptor tables and the bootloader physical direct map.
pub fn take_ownership<const N: usize>(offset: u64, pmm: &mut PhysicalMemory<N>) {
    assert!(!x86_64::instructions::interrupts::are_enabled());
    let cr4 = Cr4::read();
    assert!(
        !cr4.contains(Cr4Flags::L5_PAGING),
        "five-level paging is not supported"
    );
    assert!(
        !cr4.contains(Cr4Flags::PCID),
        "bootstrap PCID is not supported"
    );
    let (old_root, cache_flags) = Cr3::read();
    let before = pmm.free_bytes();
    let owned = clone_root::<1024>(
        &mut BootTables { offset, pmm },
        old_root.start_address().as_u64(),
    )
    .expect("cannot acquire Generic page tables");
    assert_eq!(
        before - pmm.free_bytes(),
        owned.table_frames as u64 * PAGE_SIZE
    );
    assert_ne!(owned.physical, old_root.start_address().as_u64());
    let frame = PhysFrame::from_start_address(PhysAddr::new(owned.physical)).unwrap();
    // SAFETY: the complete tree has been cloned before activation, preserving
    // all current code/data/stack mappings. Old tables stay reserved. Clearing
    // PGE flushes global translations too; no other CPU can still use this CR3.
    unsafe {
        Cr4::write(cr4 & !Cr4Flags::PAGE_GLOBAL);
        Cr3::write(frame, cache_flags);
        Cr4::write(cr4);
        Cr0::write(Cr0::read() | Cr0Flags::WRITE_PROTECT);
    }
    assert_eq!(Cr3::read().0, frame);
    assert!(Cr0::read().contains(Cr0Flags::WRITE_PROTECT));
    crate::log!(
        "[ok] Generic-owned CR3 {:#x}, {} private page-table frames, WP enabled\n",
        owned.physical,
        owned.table_frames
    );
}

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
    assert!(
        heap_size >= PAGE_SIZE,
        "heap must contain at least one page"
    );

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
    let end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(heap_end - PAGE_SIZE));

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
        zero_physical_frame(physical_memory_offset, physical);
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

unsafe fn current_offset_page_table(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
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
