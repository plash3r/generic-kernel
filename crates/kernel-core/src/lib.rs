#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod block;
pub mod genericfs;
pub mod page_tables;
pub mod physical;
pub mod vfs;
pub use physical::{PhysicalMemory, PhysicalMemoryError};

pub const PAGE_SIZE: u64 = 4096;

/// A physical byte range [start, end) already classified as usable by the loader.
#[derive(Clone, Copy, Debug, Default)]
pub struct Region {
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MapError {
    ReversedRange,
    OverlapOrUnsorted,
}

/// Bootstrap allocator kept for tiny pre-PMM users and regression coverage.
///
/// New kernel boot code should transition to PhysicalMemory, which owns its
/// metadata and supports freeing and coalescing.
pub struct FrameAllocator<'a> {
    regions: &'a [Region],
    index: usize,
    next: u64,
}

impl<'a> FrameAllocator<'a> {
    pub fn new(regions: &'a [Region]) -> Result<Self, MapError> {
        let mut previous_end = 0;
        for region in regions {
            if region.end < region.start {
                return Err(MapError::ReversedRange);
            }
            if region.start < previous_end {
                return Err(MapError::OverlapOrUnsorted);
            }
            previous_end = region.end;
        }
        Ok(Self {
            regions,
            index: 0,
            next: PAGE_SIZE,
        })
    }

    pub fn allocate(&mut self) -> Option<u64> {
        while let Some(region) = self.regions.get(self.index) {
            let start = self.next.max(region.start).max(PAGE_SIZE);
            let aligned = start
                .checked_add(PAGE_SIZE - 1)
                .map(|value| value & !(PAGE_SIZE - 1));
            if let Some(frame) = aligned {
                if let Some(end) = frame.checked_add(PAGE_SIZE) {
                    if end <= region.end {
                        self.next = end;
                        return Some(frame);
                    }
                }
            }
            self.index += 1;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_inward_and_never_allocates_null_frame() {
        let regions = [
            Region {
                start: 0,
                end: 4097,
            },
            Region {
                start: 8193,
                end: 20479,
            },
        ];
        let mut frames = FrameAllocator::new(&regions).unwrap();
        assert_eq!(frames.allocate(), Some(12288));
        assert_eq!(frames.allocate(), None);
        assert_eq!(frames.allocate(), None);
    }

    #[test]
    fn crosses_holes_without_reuse() {
        let regions = [
            Region {
                start: 4096,
                end: 12288,
            },
            Region {
                start: 32768,
                end: 36864,
            },
        ];
        let mut frames = FrameAllocator::new(&regions).unwrap();
        assert_eq!(frames.allocate(), Some(4096));
        assert_eq!(frames.allocate(), Some(8192));
        assert_eq!(frames.allocate(), Some(32768));
        assert_eq!(frames.allocate(), None);
    }

    #[test]
    fn rejects_invalid_maps() {
        assert!(matches!(
            FrameAllocator::new(&[Region { start: 2, end: 1 }]),
            Err(MapError::ReversedRange)
        ));
        assert!(matches!(
            FrameAllocator::new(&[
                Region {
                    start: 4096,
                    end: 16384
                },
                Region {
                    start: 8192,
                    end: 32768
                }
            ]),
            Err(MapError::OverlapOrUnsorted)
        ));
    }

    #[test]
    fn handles_empty_and_address_overflow() {
        assert_eq!(FrameAllocator::new(&[]).unwrap().allocate(), None);
        assert_eq!(
            FrameAllocator::new(&[Region {
                start: u64::MAX - 1024,
                end: u64::MAX
            }])
            .unwrap()
            .allocate(),
            None
        );
    }

    #[test]
    fn many_frames_are_aligned_unique_and_within_bounds() {
        let regions = [Region {
            start: 5001,
            end: 8 * 1024 * 1024 + 17,
        }];
        let mut frames = FrameAllocator::new(&regions).unwrap();
        let mut previous = 0;
        let mut count = 0;
        while let Some(frame) = frames.allocate() {
            assert_eq!(frame % PAGE_SIZE, 0);
            assert!(frame > previous && frame >= regions[0].start);
            assert!(frame + PAGE_SIZE <= regions[0].end);
            previous = frame;
            count += 1;
        }
        assert_eq!(count, 2046);
    }
}
