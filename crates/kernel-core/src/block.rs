use core::fmt;

pub const SECTOR_SIZE: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError {
    OutOfRange,
    ReadOnly,
    Io,
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OutOfRange => "block address out of range",
            Self::ReadOnly => "block device is read-only",
            Self::Io => "block I/O error",
        })
    }
}

/// Minimal synchronous sector device used below the Generic VFS.
pub trait BlockDevice: Send + Sync {
    fn sector_count(&self) -> u64;
    fn read_sector(
        &self,
        sector: u64,
        buffer: &mut [u8; SECTOR_SIZE],
    ) -> Result<(), BlockError>;
    fn write_sector(
        &self,
        sector: u64,
        buffer: &[u8; SECTOR_SIZE],
    ) -> Result<(), BlockError>;
}
