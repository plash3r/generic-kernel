use crate::{Region, PAGE_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalMemoryError {
    ReversedRange,
    OverlapOrUnsorted,
    Capacity,
    InvalidRequest,
    InvalidFree,
    Overflow,
}

/// A permanent physical-memory allocator backed by sorted free ranges.
///
/// The allocator owns its metadata instead of borrowing the firmware map, so it
/// remains valid after early boot. It supports page-aligned allocation, aligned
/// multi-page allocation, freeing and adjacent-range coalescing without a heap.
pub struct PhysicalMemory<const N: usize> {
    managed: [Region; N],
    managed_len: usize,
    free: [Region; N],
    free_len: usize,
    total_bytes: u64,
    free_bytes: u64,
}

impl<const N: usize> PhysicalMemory<N> {
    pub fn new(regions: &[Region]) -> Result<Self, PhysicalMemoryError> {
        let mut memory = Self {
            managed: [Region::default(); N],
            managed_len: 0,
            free: [Region::default(); N],
            free_len: 0,
            total_bytes: 0,
            free_bytes: 0,
        };

        let mut previous_end = 0u64;
        for region in regions {
            if region.end < region.start {
                return Err(PhysicalMemoryError::ReversedRange);
            }
            if region.start < previous_end {
                return Err(PhysicalMemoryError::OverlapOrUnsorted);
            }
            previous_end = region.end;

            let start = align_up(region.start.max(PAGE_SIZE), PAGE_SIZE)
                .ok_or(PhysicalMemoryError::Overflow)?;
            let end = align_down(region.end, PAGE_SIZE);
            if start >= end {
                continue;
            }

            memory.push_initial(Region { start, end })?;
        }

        Ok(memory)
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub fn free_bytes(&self) -> u64 {
        self.free_bytes
    }

    pub fn used_bytes(&self) -> u64 {
        self.total_bytes - self.free_bytes
    }

    pub fn managed_region_count(&self) -> usize {
        self.managed_len
    }

    pub fn free_region_count(&self) -> usize {
        self.free_len
    }

    pub fn allocate_frame(&mut self) -> Result<Option<u64>, PhysicalMemoryError> {
        self.allocate_pages(1, 1)
    }

    pub fn allocate_pages(
        &mut self,
        pages: u64,
        align_pages: u64,
    ) -> Result<Option<u64>, PhysicalMemoryError> {
        if pages == 0 || align_pages == 0 || !align_pages.is_power_of_two() {
            return Err(PhysicalMemoryError::InvalidRequest);
        }

        let size = pages
            .checked_mul(PAGE_SIZE)
            .ok_or(PhysicalMemoryError::Overflow)?;
        let alignment = align_pages
            .checked_mul(PAGE_SIZE)
            .ok_or(PhysicalMemoryError::Overflow)?;

        for index in 0..self.free_len {
            let region = self.free[index];
            let Some(start) = align_up(region.start, alignment) else {
                continue;
            };
            let Some(end) = start.checked_add(size) else {
                continue;
            };
            if end > region.end {
                continue;
            }

            let has_prefix = start > region.start;
            let has_suffix = end < region.end;
            if has_prefix && has_suffix && self.free_len == N {
                return Err(PhysicalMemoryError::Capacity);
            }

            match (has_prefix, has_suffix) {
                (false, false) => self.remove_free(index),
                (false, true) => self.free[index].start = end,
                (true, false) => self.free[index].end = start,
                (true, true) => {
                    let suffix = Region {
                        start: end,
                        end: region.end,
                    };
                    self.free[index].end = start;
                    self.insert_free(index + 1, suffix)?;
                }
            }

            self.free_bytes -= size;
            return Ok(Some(start));
        }

        Ok(None)
    }

    pub fn free_pages(
        &mut self,
        start: u64,
        pages: u64,
    ) -> Result<(), PhysicalMemoryError> {
        if pages == 0 || start < PAGE_SIZE || start % PAGE_SIZE != 0 {
            return Err(PhysicalMemoryError::InvalidFree);
        }

        let size = pages
            .checked_mul(PAGE_SIZE)
            .ok_or(PhysicalMemoryError::Overflow)?;
        let end = start
            .checked_add(size)
            .ok_or(PhysicalMemoryError::Overflow)?;
        if end % PAGE_SIZE != 0 || !self.is_managed(start, end) {
            return Err(PhysicalMemoryError::InvalidFree);
        }

        let mut index = 0usize;
        while index < self.free_len && self.free[index].start < start {
            index += 1;
        }

        if index > 0 && self.free[index - 1].end > start {
            return Err(PhysicalMemoryError::InvalidFree);
        }
        if index < self.free_len && self.free[index].start < end {
            return Err(PhysicalMemoryError::InvalidFree);
        }

        let merge_left = index > 0 && self.free[index - 1].end == start;
        let merge_right = index < self.free_len && self.free[index].start == end;

        match (merge_left, merge_right) {
            (true, true) => {
                self.free[index - 1].end = self.free[index].end;
                self.remove_free(index);
            }
            (true, false) => self.free[index - 1].end = end,
            (false, true) => self.free[index].start = start,
            (false, false) => self.insert_free(index, Region { start, end })?,
        }

        self.free_bytes = self
            .free_bytes
            .checked_add(size)
            .ok_or(PhysicalMemoryError::Overflow)?;
        if self.free_bytes > self.total_bytes {
            return Err(PhysicalMemoryError::InvalidFree);
        }
        Ok(())
    }

    fn push_initial(&mut self, region: Region) -> Result<(), PhysicalMemoryError> {
        if self.managed_len > 0 && self.managed[self.managed_len - 1].end == region.start {
            self.managed[self.managed_len - 1].end = region.end;
            self.free[self.free_len - 1].end = region.end;
        } else {
            if self.managed_len == N || self.free_len == N {
                return Err(PhysicalMemoryError::Capacity);
            }
            self.managed[self.managed_len] = region;
            self.managed_len += 1;
            self.free[self.free_len] = region;
            self.free_len += 1;
        }

        let bytes = region.end - region.start;
        self.total_bytes = self
            .total_bytes
            .checked_add(bytes)
            .ok_or(PhysicalMemoryError::Overflow)?;
        self.free_bytes = self
            .free_bytes
            .checked_add(bytes)
            .ok_or(PhysicalMemoryError::Overflow)?;
        Ok(())
    }

    fn is_managed(&self, start: u64, end: u64) -> bool {
        self.managed[..self.managed_len]
            .iter()
            .any(|region| region.start <= start && end <= region.end)
    }

    fn insert_free(
        &mut self,
        index: usize,
        region: Region,
    ) -> Result<(), PhysicalMemoryError> {
        if self.free_len == N {
            return Err(PhysicalMemoryError::Capacity);
        }
        for slot in (index..self.free_len).rev() {
            self.free[slot + 1] = self.free[slot];
        }
        self.free[index] = region;
        self.free_len += 1;
        Ok(())
    }

    fn remove_free(&mut self, index: usize) {
        for slot in index..self.free_len - 1 {
            self.free[slot] = self.free[slot + 1];
        }
        self.free_len -= 1;
        if self.free_len < N {
            self.free[self.free_len] = Region::default();
        }
    }
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    debug_assert!(alignment.is_power_of_two());
    value
        .checked_add(alignment - 1)
        .map(|rounded| rounded & !(alignment - 1))
}

fn align_down(value: u64, alignment: u64) -> u64 {
    debug_assert!(alignment.is_power_of_two());
    value & !(alignment - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_regions_and_excludes_null_page() {
        let memory = PhysicalMemory::<8>::new(&[
            Region {
                start: 0,
                end: 5000,
            },
            Region {
                start: 8193,
                end: 20000,
            },
        ])
        .unwrap();

        assert_eq!(memory.total_bytes(), 3 * PAGE_SIZE);
        assert_eq!(memory.free_bytes(), 3 * PAGE_SIZE);
        assert_eq!(memory.managed_region_count(), 2);
    }

    #[test]
    fn allocates_frees_and_coalesces() {
        let mut memory = PhysicalMemory::<8>::new(&[Region {
            start: PAGE_SIZE,
            end: PAGE_SIZE * 9,
        }])
        .unwrap();

        let a = memory.allocate_frame().unwrap().unwrap();
        let b = memory.allocate_frame().unwrap().unwrap();
        let c = memory.allocate_pages(2, 1).unwrap().unwrap();
        assert_eq!(a, PAGE_SIZE);
        assert_eq!(b, PAGE_SIZE * 2);
        assert_eq!(c, PAGE_SIZE * 3);
        assert_eq!(memory.used_bytes(), PAGE_SIZE * 4);

        memory.free_pages(b, 1).unwrap();
        memory.free_pages(a, 1).unwrap();
        memory.free_pages(c, 2).unwrap();

        assert_eq!(memory.free_bytes(), memory.total_bytes());
        assert_eq!(memory.free_region_count(), 1);
    }

    #[test]
    fn honors_multi_page_alignment() {
        let mut memory = PhysicalMemory::<8>::new(&[Region {
            start: PAGE_SIZE,
            end: PAGE_SIZE * 32,
        }])
        .unwrap();

        let address = memory.allocate_pages(2, 4).unwrap().unwrap();
        assert_eq!(address % (PAGE_SIZE * 4), 0);
        assert_eq!(memory.used_bytes(), PAGE_SIZE * 2);
    }

    #[test]
    fn rejects_double_free_and_out_of_range_free() {
        let mut memory = PhysicalMemory::<8>::new(&[Region {
            start: PAGE_SIZE,
            end: PAGE_SIZE * 8,
        }])
        .unwrap();

        let address = memory.allocate_frame().unwrap().unwrap();
        memory.free_pages(address, 1).unwrap();
        assert_eq!(
            memory.free_pages(address, 1),
            Err(PhysicalMemoryError::InvalidFree)
        );
        assert_eq!(
            memory.free_pages(PAGE_SIZE * 100, 1),
            Err(PhysicalMemoryError::InvalidFree)
        );
    }

    #[test]
    fn rejects_bad_maps_and_requests() {
        assert!(matches!(
            PhysicalMemory::<8>::new(&[Region { start: 2, end: 1 }]),
            Err(PhysicalMemoryError::ReversedRange)
        ));
        assert!(matches!(
            PhysicalMemory::<8>::new(&[
                Region {
                    start: PAGE_SIZE,
                    end: PAGE_SIZE * 4,
                },
                Region {
                    start: PAGE_SIZE * 3,
                    end: PAGE_SIZE * 5,
                },
            ]),
            Err(PhysicalMemoryError::OverlapOrUnsorted)
        ));

        let mut memory = PhysicalMemory::<8>::new(&[Region {
            start: PAGE_SIZE,
            end: PAGE_SIZE * 8,
        }])
        .unwrap();
        assert_eq!(
            memory.allocate_pages(0, 1),
            Err(PhysicalMemoryError::InvalidRequest)
        );
        assert_eq!(
            memory.allocate_pages(1, 3),
            Err(PhysicalMemoryError::InvalidRequest)
        );
    }
}
