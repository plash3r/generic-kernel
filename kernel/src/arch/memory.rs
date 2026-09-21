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
    let owned = clone_root::<16384>(
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

#[derive(Clone, Copy, Debug)]
pub struct RuntimeMemoryDiagnostics {
    pub cr3: u64,
    pub write_protect: bool,
    pub heap_start_mapped: bool,
    pub heap_end_mapped: bool,
    pub lower_guard_unmapped: bool,
    pub upper_guard_unmapped: bool,
}

pub fn runtime_diagnostics(
    physical_memory_offset: u64,
    heap_start: u64,
    heap_size: u64,
) -> RuntimeMemoryDiagnostics {
    let heap_end = heap_start.saturating_add(heap_size);
    let lower_guard = heap_start.saturating_sub(PAGE_SIZE);
    let upper_guard = heap_end;
    let last_heap_page = heap_end.saturating_sub(PAGE_SIZE);

    let mapper = unsafe { current_offset_page_table(VirtAddr::new(physical_memory_offset)) };
    let (root, _) = Cr3::read();

    RuntimeMemoryDiagnostics {
        cr3: root.start_address().as_u64(),
        write_protect: Cr0::read().contains(Cr0Flags::WRITE_PROTECT),
        heap_start_mapped: mapper.translate_addr(VirtAddr::new(heap_start)).is_some(),
        heap_end_mapped: mapper
            .translate_addr(VirtAddr::new(last_heap_page))
            .is_some(),
        lower_guard_unmapped: mapper.translate_addr(VirtAddr::new(lower_guard)).is_none(),
        upper_guard_unmapped: mapper.translate_addr(VirtAddr::new(upper_guard)).is_none(),
    }
}

pub fn map_mmio<const N: usize>(
    physical_memory_offset: u64,
    pmm: &mut PhysicalMemory<N>,
    virtual_start: u64,
    physical_start: u64,
    pages: u64,
) -> Result<(), &'static str> {
    if pages == 0 || virtual_start % PAGE_SIZE != 0 || physical_start % PAGE_SIZE != 0 {
        return Err("MMIO mapping must be non-empty and page aligned");
    }

    let physical_offset = VirtAddr::new(physical_memory_offset);
    // SAFETY: Generic owns the active table tree and the direct map gives access
    // to all page-table frames used by the mapper.
    let mut mapper = unsafe { current_offset_page_table(physical_offset) };
    let mut allocator = PmmFrameAllocator::new(pmm);
    let flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::NO_EXECUTE
        | PageTableFlags::NO_CACHE
        | PageTableFlags::WRITE_THROUGH;

    for index in 0..pages {
        let virtual_address = virtual_start
            .checked_add(index * PAGE_SIZE)
            .ok_or("MMIO virtual-address overflow")?;
        let physical_address = physical_start
            .checked_add(index * PAGE_SIZE)
            .ok_or("MMIO physical-address overflow")?;
        let page = Page::<Size4KiB>::from_start_address(VirtAddr::new(virtual_address))
            .map_err(|_| "unaligned MMIO virtual page")?;
        let frame = PhysFrame::<Size4KiB>::from_start_address(PhysAddr::new(physical_address))
            .map_err(|_| "unaligned MMIO physical frame")?;

        if mapper.translate_addr(page.start_address()).is_some() {
            return Err("MMIO virtual range is already mapped");
        }

        // SAFETY: the virtual page was checked unused, the physical frame is an
        // explicitly requested device MMIO frame, and page-table allocations are
        // sourced from Generic's PMM.
        unsafe {
            mapper
                .map_to(page, frame, flags, &mut allocator)
                .map_err(|_| "failed to map MMIO page")?
                .flush();
        }
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AddressSpace {
    pub root: u64,
}

pub const USER_ADDRESS_LIMIT: u64 = 1u64 << 39;

pub fn active_root() -> u64 {
    Cr3::read().0.start_address().as_u64()
}

/// Creates a fully private process page-table tree by deep-cloning the active
/// kernel address space. Leaf physical mappings and permissions are preserved,
/// while every page-table frame is process-owned. User mappings added later
/// therefore cannot mutate the kernel's page-table tree.
///
/// This is intentionally conservative. A future optimization can share
/// supervisor-only kernel branches once the permanent kernel virtual layout is
/// fixed and page-table ownership/refcounting is available.
pub fn create_user_address_space<const N: usize>(
    physical_memory_offset: u64,
    pmm: &mut PhysicalMemory<N>,
) -> Result<AddressSpace, &'static str> {
    let kernel_root = active_root();
    let before = pmm.free_bytes();
    let owned = clone_root::<16384>(
        &mut BootTables {
            offset: physical_memory_offset,
            pmm,
        },
        kernel_root,
    )
    .map_err(|_| "failed to clone process page-table tree")?;

    let consumed = before
        .checked_sub(pmm.free_bytes())
        .ok_or("process page-table accounting underflow")?;
    if consumed != owned.table_frames as u64 * PAGE_SIZE {
        return Err("process page-table accounting mismatch");
    }

    Ok(AddressSpace {
        root: owned.physical,
    })
}

pub fn activate_address_space(address_space: AddressSpace) -> Result<(), &'static str> {
    let frame = PhysFrame::<Size4KiB>::from_start_address(PhysAddr::new(address_space.root))
        .map_err(|_| "process CR3 is not page aligned")?;
    let (_, flags) = Cr3::read();
    // SAFETY: AddressSpace roots are created from the active kernel mappings,
    // so kernel code, stacks, interrupt state and the physical direct map stay
    // valid across the CR3 transition.
    unsafe {
        Cr3::write(frame, flags);
    }
    Ok(())
}

pub fn map_user_page<const N: usize>(
    physical_memory_offset: u64,
    pmm: &mut PhysicalMemory<N>,
    address_space: AddressSpace,
    virtual_address: u64,
    writable: bool,
    executable: bool,
    initial: &[u8],
) -> Result<u64, &'static str> {
    if virtual_address % PAGE_SIZE != 0 {
        return Err("user virtual address must be page aligned");
    }
    if initial.len() > PAGE_SIZE as usize {
        return Err("initial user page data exceeds one page");
    }
    if virtual_address >= USER_ADDRESS_LIMIT {
        return Err("user virtual address exceeds Generic userspace limit");
    }

    let physical_offset = VirtAddr::new(physical_memory_offset);
    // SAFETY: the supplied root is a process-owned PML4 reachable through the
    // direct map. Its first slot is private to this address space.
    let mut mapper = unsafe { offset_page_table_for_root(address_space.root, physical_offset)? };
    let page = Page::<Size4KiB>::from_start_address(VirtAddr::new(virtual_address))
        .map_err(|_| "unaligned user page")?;
    if mapper.translate_addr(page.start_address()).is_some() {
        return Err("user virtual page is already mapped");
    }

    let mut allocator = PmmFrameAllocator::new(pmm);
    let physical = allocator
        .allocate_physical()
        .ok_or("out of physical memory for user page")?;
    zero_physical_frame(physical_memory_offset, physical);

    if !initial.is_empty() {
        let direct = physical_memory_offset
            .checked_add(physical)
            .ok_or("user direct-map address overflow")?;
        // SAFETY: the newly allocated physical frame is uniquely owned and
        // initial.len() was checked to fit inside the page.
        unsafe {
            core::ptr::copy_nonoverlapping(
                initial.as_ptr(),
                VirtAddr::new(direct).as_mut_ptr::<u8>(),
                initial.len(),
            );
        }
    }

    let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
    if writable {
        flags |= PageTableFlags::WRITABLE;
    }
    if !executable {
        flags |= PageTableFlags::NO_EXECUTE;
    }

    let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(physical));
    // SAFETY: the page is confirmed unmapped in this process address space,
    // the frame is uniquely allocated from PMM, and USER_ACCESSIBLE is
    // intentionally confined to private PML4 slot 0.
    unsafe {
        mapper
            .map_to(page, frame, flags, &mut allocator)
            .map_err(|_| "failed to map user page")?
            .flush();
    }

    Ok(physical)
}

pub fn user_range_accessible(
    physical_memory_offset: u64,
    address: u64,
    length: usize,
    writable: bool,
) -> bool {
    if length == 0 {
        return address < USER_ADDRESS_LIMIT;
    }

    let Some(last) = address.checked_add(length as u64 - 1) else {
        return false;
    };
    if address >= USER_ADDRESS_LIMIT || last >= USER_ADDRESS_LIMIT {
        return false;
    }

    let root = active_root();
    let first_page = address & !(PAGE_SIZE - 1);
    let last_page = last & !(PAGE_SIZE - 1);
    let mut page = first_page;

    loop {
        if !user_page_accessible(physical_memory_offset, root, page, writable) {
            return false;
        }
        if page == last_page {
            break;
        }
        let Some(next) = page.checked_add(PAGE_SIZE) else {
            return false;
        };
        page = next;
    }

    true
}

fn user_page_accessible(
    physical_memory_offset: u64,
    root: u64,
    virtual_address: u64,
    writable: bool,
) -> bool {
    const PRESENT: u64 = 1 << 0;
    const WRITABLE: u64 = 1 << 1;
    const USER: u64 = 1 << 2;
    const HUGE: u64 = 1 << 7;
    const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;

    let indexes = [
        ((virtual_address >> 39) & 0x1ff) as usize,
        ((virtual_address >> 30) & 0x1ff) as usize,
        ((virtual_address >> 21) & 0x1ff) as usize,
        ((virtual_address >> 12) & 0x1ff) as usize,
    ];

    let mut table = root;
    for (level, index) in indexes.into_iter().enumerate() {
        let Ok(pointer) = page_table_entry_pointer(physical_memory_offset, table, index) else {
            return false;
        };
        // SAFETY: pointer addresses a live page-table entry through the
        // physical direct map. The single bootstrap CPU owns page-table edits.
        let entry = unsafe { pointer.read_volatile() };
        if entry & PRESENT == 0 || entry & USER == 0 {
            return false;
        }
        if writable && entry & WRITABLE == 0 {
            return false;
        }

        if level == 3 || (level >= 1 && entry & HUGE != 0) {
            return true;
        }
        table = entry & ADDRESS_MASK;
        if table == 0 {
            return false;
        }
    }

    false
}

fn page_table_entry_pointer(
    physical_memory_offset: u64,
    table_physical: u64,
    index: usize,
) -> Result<*mut u64, &'static str> {
    if index >= 512 || table_physical % PAGE_SIZE != 0 {
        return Err("invalid page-table entry");
    }
    let address = physical_memory_offset
        .checked_add(table_physical)
        .and_then(|base| base.checked_add((index * core::mem::size_of::<u64>()) as u64))
        .ok_or("page-table direct-map overflow")?;
    Ok(VirtAddr::new(address).as_mut_ptr::<u64>())
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

unsafe fn offset_page_table_for_root(
    root: u64,
    physical_memory_offset: VirtAddr,
) -> Result<OffsetPageTable<'static>, &'static str> {
    let frame = PhysFrame::<Size4KiB>::from_start_address(PhysAddr::new(root))
        .map_err(|_| "address-space root is not page aligned")?;
    let virtual_address = physical_memory_offset + frame.start_address().as_u64();
    let page_table_ptr: *mut PageTable = virtual_address.as_mut_ptr();

    // SAFETY: caller guarantees root is a live process-owned PML4 and the
    // physical direct map is valid for all page-table frames it references.
    let level_4_table = unsafe { &mut *page_table_ptr };
    // SAFETY: the direct-map offset satisfies OffsetPageTable's translation
    // contract for every physical frame.
    Ok(unsafe { OffsetPageTable::new(level_4_table, physical_memory_offset) })
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
