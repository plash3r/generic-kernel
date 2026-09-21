//! Testable x86_64 four-level page-table handoff. No physical pointers here.

const PRESENT: u64 = 1;
const HUGE: u64 = 1 << 7;
const ADDRESS: u64 = 0x000f_ffff_ffff_f000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneError {
    OutOfMemory,
    TableLimit,
    InvalidFrame,
    InvalidHugePage,
    Cycle,
}

/// Implementations must return fresh, exclusively owned frames from allocate,
/// disjoint from the source tables. Reads/writes address initialized tables.
/// release must accept every allocated frame, including partial clones.
pub trait TableMemory {
    fn allocate(&mut self) -> Option<u64>;
    fn release(&mut self, frame: u64);
    fn read(&self, frame: u64, index: usize) -> u64;
    fn write(&mut self, frame: u64, index: usize, entry: u64);
}

#[derive(Clone, Copy, Debug)]
pub struct OwnedRoot {
    pub physical: u64,
    pub table_frames: usize,
}

/// Deep-copy every present table, retaining leaf addresses and ALL flags.
/// 1 GiB/2 MiB leaves are not descended into; bit 7 at level 1 is PAT.
/// A failed attempt frees all allocations and never writes to the source.
/// Recursive mappings are rejected; Generic uses a physical direct map instead.
pub fn clone_root<const LIMIT: usize>(
    memory: &mut impl TableMemory,
    source: u64,
) -> Result<OwnedRoot, CloneError> {
    let mut allocated = [0u64; LIMIT];
    let mut count = 0;
    let mut ancestors = [0u64; 4];
    let result = clone_table(
        memory,
        source,
        4,
        &mut ancestors,
        &mut allocated,
        &mut count,
    );
    match result {
        Ok(physical) => Ok(OwnedRoot {
            physical,
            table_frames: count,
        }),
        Err(error) => {
            for frame in allocated[..count].iter().rev() {
                memory.release(*frame);
            }
            Err(error)
        }
    }
}

fn clone_table<const LIMIT: usize>(
    memory: &mut impl TableMemory,
    source: u64,
    level: usize,
    ancestors: &mut [u64; 4],
    allocated: &mut [u64; LIMIT],
    count: &mut usize,
) -> Result<u64, CloneError> {
    if source == 0 || source & !ADDRESS != 0 {
        return Err(CloneError::InvalidFrame);
    }
    let depth = 4 - level;
    if ancestors[..depth].contains(&source) {
        return Err(CloneError::Cycle);
    }
    ancestors[depth] = source;
    if *count == LIMIT {
        return Err(CloneError::TableLimit);
    }
    let destination = memory.allocate().ok_or(CloneError::OutOfMemory)?;
    allocated[*count] = destination;
    *count += 1;
    if destination == 0 || destination & !ADDRESS != 0 {
        return Err(CloneError::InvalidFrame);
    }
    for index in 0..512 {
        let entry = memory.read(source, index);
        let mut replacement = entry;
        if entry & PRESENT != 0 && level > 1 {
            if entry & HUGE != 0 {
                if level == 4 {
                    return Err(CloneError::InvalidHugePage);
                }
                // For large leaves bit 12 is PAT, not an address bit.
                let reserved = if level == 3 { 0x3fff_e000 } else { 0x1f_e000 };
                if entry & reserved != 0 {
                    return Err(CloneError::InvalidHugePage);
                }
            } else {
                let child = clone_table(
                    memory,
                    entry & ADDRESS,
                    level - 1,
                    ancestors,
                    allocated,
                    count,
                )?;
                replacement = (entry & !ADDRESS) | child;
            }
        }
        memory.write(destination, index, replacement);
    }
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Memory {
        tables: BTreeMap<u64, [u64; 512]>,
        next: u64,
        budget: usize,
    }

    impl Memory {
        fn fixture() -> Self {
            let mut tables = BTreeMap::new();
            for frame in [0x1000, 0x2000, 0x3000, 0x4000] {
                tables.insert(frame, [0; 512]);
            }
            tables.get_mut(&0x1000).unwrap()[256] = 0x2000 | 0x23;
            tables.get_mut(&0x2000).unwrap()[3] = 0x3000 | 3;
            tables.get_mut(&0x3000).unwrap()[5] = 0x4000 | 3;
            // NX + RW + GLOBAL + PAT on a 4 KiB leaf.
            tables.get_mut(&0x4000).unwrap()[7] = (1 << 63) | 0x9000 | 0x183;
            // 1 GiB and 2 MiB leaves with large-page PAT set.
            tables.get_mut(&0x2000).unwrap()[4] = 0x4000_0000 | 0x1083;
            tables.get_mut(&0x3000).unwrap()[6] = 0x20_0000 | 0x1083;
            tables.get_mut(&0x4000).unwrap()[8] = 0xdead_0000; // non-present metadata
            Self {
                tables,
                next: 0x10_0000,
                budget: usize::MAX,
            }
        }
    }

    impl TableMemory for Memory {
        fn allocate(&mut self) -> Option<u64> {
            if self.budget == 0 {
                return None;
            }
            self.budget -= 1;
            let frame = self.next;
            self.next += 4096;
            assert!(self.tables.insert(frame, [0; 512]).is_none());
            Some(frame)
        }
        fn release(&mut self, frame: u64) {
            self.tables.remove(&frame).unwrap();
        }
        fn read(&self, frame: u64, index: usize) -> u64 {
            self.tables[&frame][index]
        }
        fn write(&mut self, frame: u64, index: usize, entry: u64) {
            self.tables.get_mut(&frame).unwrap()[index] = entry;
        }
    }

    #[test]
    fn owns_every_table_and_preserves_leaves_and_permissions() {
        let mut memory = Memory::fixture();
        let original = memory.tables.clone();
        let root = clone_root::<16>(&mut memory, 0x1000).unwrap();
        assert_eq!(root.table_frames, 4);
        let l3 = memory.read(root.physical, 256) & ADDRESS;
        let l2 = memory.read(l3, 3) & ADDRESS;
        let l1 = memory.read(l2, 5) & ADDRESS;
        for frame in [root.physical, l3, l2, l1] {
            assert!(!original.contains_key(&frame));
        }
        assert_eq!(memory.read(root.physical, 256) & !ADDRESS, 0x23);
        assert_eq!(memory.read(l3, 4), original[&0x2000][4]);
        assert_eq!(memory.read(l2, 6), original[&0x3000][6]);
        assert_eq!(memory.tables[&l1], original[&0x4000]);
        memory.write(l1, 7, 0);
        for (frame, table) in &original {
            assert_eq!(&memory.tables[frame], table);
        }
    }

    #[test]
    fn exhaustion_rolls_back_at_every_depth() {
        for budget in 0..4 {
            let mut memory = Memory::fixture();
            let original = memory.tables.clone();
            memory.budget = budget;
            assert_eq!(
                clone_root::<16>(&mut memory, 0x1000).unwrap_err(),
                CloneError::OutOfMemory
            );
            assert_eq!(memory.tables, original);
        }
    }

    #[test]
    fn allocation_limit_rolls_back() {
        let mut memory = Memory::fixture();
        let original = memory.tables.clone();
        assert_eq!(
            clone_root::<2>(&mut memory, 0x1000).unwrap_err(),
            CloneError::TableLimit
        );
        assert_eq!(memory.tables, original);
    }

    #[test]
    fn rejects_cycles_and_bad_large_pages_without_touching_source() {
        for (frame, index, entry, expected) in [
            (0x1000, 510, 0x1003, CloneError::Cycle),
            (0x1000, 0, 0x83, CloneError::InvalidHugePage),
            (0x2000, 4, 0x4000_2083, CloneError::InvalidHugePage),
            (0x3000, 6, 0x20_2083, CloneError::InvalidHugePage),
        ] {
            let mut memory = Memory::fixture();
            memory.write(frame, index, entry);
            let original = memory.tables.clone();
            assert_eq!(clone_root::<16>(&mut memory, 0x1000).unwrap_err(), expected);
            assert_eq!(memory.tables, original);
        }
    }

    #[test]
    fn rejects_invalid_root_before_allocation() {
        for root in [0, 1, 0x1001, 1 << 60] {
            let mut memory = Memory::fixture();
            let original = memory.tables.clone();
            assert_eq!(
                clone_root::<16>(&mut memory, root).unwrap_err(),
                CloneError::InvalidFrame
            );
            assert_eq!(memory.tables, original);
        }
    }
}
