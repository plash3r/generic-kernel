use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::null_mut,
};
use spin::Mutex;

const MAX_FREE_RANGES: usize = 256;

#[derive(Clone, Copy, Default)]
struct Range {
    start: usize,
    end: usize,
}

struct HeapState {
    initialized: bool,
    start: usize,
    end: usize,
    free: [Range; MAX_FREE_RANGES],
    free_len: usize,
    free_bytes: usize,
}

impl HeapState {
    const fn empty() -> Self {
        Self {
            initialized: false,
            start: 0,
            end: 0,
            free: [Range { start: 0, end: 0 }; MAX_FREE_RANGES],
            free_len: 0,
            free_bytes: 0,
        }
    }

    fn init(&mut self, start: usize, size: usize) {
        assert!(!self.initialized, "kernel heap initialized twice");
        let end = start.checked_add(size).expect("kernel heap address overflow");
        assert!(size > 0, "kernel heap must not be empty");

        self.initialized = true;
        self.start = start;
        self.end = end;
        self.free[0] = Range { start, end };
        self.free_len = 1;
        self.free_bytes = size;
    }

    fn allocate(&mut self, layout: Layout) -> Option<usize> {
        if !self.initialized {
            return None;
        }

        let size = layout.size().max(1);
        let alignment = layout.align();

        for index in 0..self.free_len {
            let region = self.free[index];
            let start = align_up(region.start, alignment)?;
            let end = start.checked_add(size)?;
            if end > region.end {
                continue;
            }

            let has_prefix = start > region.start;
            let has_suffix = end < region.end;
            if has_prefix && has_suffix && self.free_len == MAX_FREE_RANGES {
                continue;
            }

            match (has_prefix, has_suffix) {
                (false, false) => self.remove(index),
                (false, true) => self.free[index].start = end,
                (true, false) => self.free[index].end = start,
                (true, true) => {
                    self.free[index].end = start;
                    self.insert(
                        index + 1,
                        Range {
                            start: end,
                            end: region.end,
                        },
                    )
                    .expect("kernel heap free-range metadata exhausted");
                }
            }

            self.free_bytes -= size;
            return Some(start);
        }

        None
    }

    fn deallocate(&mut self, start: usize, layout: Layout) {
        assert!(self.initialized, "kernel heap deallocation before init");
        let size = layout.size().max(1);
        let end = start
            .checked_add(size)
            .expect("kernel heap deallocation overflow");
        assert!(
            self.start <= start && end <= self.end,
            "kernel heap deallocation outside heap"
        );

        let mut index = 0usize;
        while index < self.free_len && self.free[index].start < start {
            index += 1;
        }

        if index > 0 {
            assert!(
                self.free[index - 1].end <= start,
                "kernel heap double free or overlap"
            );
        }
        if index < self.free_len {
            assert!(
                end <= self.free[index].start,
                "kernel heap double free or overlap"
            );
        }

        let merge_left = index > 0 && self.free[index - 1].end == start;
        let merge_right = index < self.free_len && self.free[index].start == end;

        match (merge_left, merge_right) {
            (true, true) => {
                self.free[index - 1].end = self.free[index].end;
                self.remove(index);
            }
            (true, false) => self.free[index - 1].end = end,
            (false, true) => self.free[index].start = start,
            (false, false) => self
                .insert(index, Range { start, end })
                .expect("kernel heap free-range metadata exhausted"),
        }

        self.free_bytes = self
            .free_bytes
            .checked_add(size)
            .expect("kernel heap free-byte counter overflow");
        assert!(
            self.free_bytes <= self.end - self.start,
            "kernel heap free-byte counter corrupted"
        );
    }

    fn insert(&mut self, index: usize, range: Range) -> Result<(), ()> {
        if self.free_len == MAX_FREE_RANGES {
            return Err(());
        }
        for slot in (index..self.free_len).rev() {
            self.free[slot + 1] = self.free[slot];
        }
        self.free[index] = range;
        self.free_len += 1;
        Ok(())
    }

    fn remove(&mut self, index: usize) {
        for slot in index..self.free_len - 1 {
            self.free[slot] = self.free[slot + 1];
        }
        self.free_len -= 1;
        self.free[self.free_len] = Range::default();
    }
}

pub struct KernelAllocator {
    state: Mutex<HeapState>,
}

impl KernelAllocator {
    const fn new() -> Self {
        Self {
            state: Mutex::new(HeapState::empty()),
        }
    }

    pub fn init(&self, start: usize, size: usize) {
        self.state.lock().init(start, size);
    }

    pub fn free_bytes(&self) -> usize {
        self.state.lock().free_bytes
    }

    pub fn total_bytes(&self) -> usize {
        let state = self.state.lock();
        if state.initialized {
            state.end - state.start
        } else {
            0
        }
    }

    pub fn verify(&self) {
        let before = self.free_bytes();
        let small = Layout::from_size_align(37, 8).expect("valid heap smoke layout");
        let page_aligned =
            Layout::from_size_align(4096, 4096).expect("valid heap smoke layout");

        // SAFETY: these allocations are paired with deallocations using the
        // exact same layouts, and both pointers are checked for allocation failure.
        unsafe {
            let first = GlobalAlloc::alloc(self, small);
            assert!(!first.is_null(), "kernel heap failed small allocation");
            core::ptr::write_bytes(first, 0xa5, small.size());

            let second = GlobalAlloc::alloc(self, page_aligned);
            assert!(
                !second.is_null(),
                "kernel heap failed page-aligned allocation"
            );
            assert_eq!((second as usize) % 4096, 0);
            core::ptr::write_bytes(second, 0x5a, page_aligned.size());

            assert_eq!(core::ptr::read_volatile(first), 0xa5);
            assert_eq!(core::ptr::read_volatile(second), 0x5a);

            GlobalAlloc::dealloc(self, second, page_aligned);
            GlobalAlloc::dealloc(self, first, small);
        }

        assert_eq!(
            self.free_bytes(),
            before,
            "kernel heap failed to coalesce smoke allocations"
        );
    }
}

// SAFETY: all mutable allocator state is serialized by the spin mutex. Returned
// memory comes only from the mapped heap range, and GlobalAlloc's caller owns
// the obligation to pair allocations with the original layout.
unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.state
            .lock()
            .allocate(layout)
            .map_or(null_mut(), |address| address as *mut u8)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.state.lock().deallocate(ptr as usize, layout);
    }
}

#[global_allocator]
pub static KERNEL_ALLOCATOR: KernelAllocator = KernelAllocator::new();

fn align_up(value: usize, alignment: usize) -> Option<usize> {
    debug_assert!(alignment.is_power_of_two());
    value
        .checked_add(alignment - 1)
        .map(|rounded| rounded & !(alignment - 1))
}
