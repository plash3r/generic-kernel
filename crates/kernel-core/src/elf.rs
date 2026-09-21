use alloc::vec::Vec;

const ELF_HEADER_SIZE: usize = 64;
const PROGRAM_HEADER_SIZE: usize = 56;
const PT_LOAD: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElfError {
    TooSmall,
    BadMagic,
    UnsupportedClass,
    UnsupportedEndian,
    UnsupportedVersion,
    UnsupportedType,
    UnsupportedMachine,
    InvalidHeader,
    TruncatedProgramHeaders,
    InvalidSegment,
    TruncatedSegment,
}

#[derive(Clone, Copy, Debug)]
pub struct LoadSegment<'a> {
    pub virtual_address: u64,
    pub memory_size: u64,
    pub file_data: &'a [u8],
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub alignment: u64,
}

#[derive(Debug)]
pub struct ElfImage<'a> {
    entry: u64,
    segments: Vec<LoadSegment<'a>>,
}

impl<'a> ElfImage<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self, ElfError> {
        if data.len() < ELF_HEADER_SIZE {
            return Err(ElfError::TooSmall);
        }
        if &data[0..4] != b"\x7fELF" {
            return Err(ElfError::BadMagic);
        }
        if data[4] != 2 {
            return Err(ElfError::UnsupportedClass);
        }
        if data[5] != 1 {
            return Err(ElfError::UnsupportedEndian);
        }
        if data[6] != 1 {
            return Err(ElfError::UnsupportedVersion);
        }
        if read_u16(data, 16)? != 2 {
            return Err(ElfError::UnsupportedType);
        }
        if read_u16(data, 18)? != 62 {
            return Err(ElfError::UnsupportedMachine);
        }
        if read_u32(data, 20)? != 1 {
            return Err(ElfError::UnsupportedVersion);
        }

        let entry = read_u64(data, 24)?;
        let phoff = usize::try_from(read_u64(data, 32)?).map_err(|_| ElfError::InvalidHeader)?;
        let ehsize = read_u16(data, 52)? as usize;
        let phentsize = read_u16(data, 54)? as usize;
        let phnum = read_u16(data, 56)? as usize;

        if ehsize < ELF_HEADER_SIZE || phentsize != PROGRAM_HEADER_SIZE {
            return Err(ElfError::InvalidHeader);
        }
        let phbytes = phentsize
            .checked_mul(phnum)
            .and_then(|bytes| phoff.checked_add(bytes))
            .ok_or(ElfError::TruncatedProgramHeaders)?;
        if phbytes > data.len() {
            return Err(ElfError::TruncatedProgramHeaders);
        }

        let mut segments = Vec::new();
        for index in 0..phnum {
            let base = phoff + index * phentsize;
            if read_u32(data, base)? != PT_LOAD {
                continue;
            }

            let flags = read_u32(data, base + 4)?;
            let offset =
                usize::try_from(read_u64(data, base + 8)?).map_err(|_| ElfError::InvalidSegment)?;
            let virtual_address = read_u64(data, base + 16)?;
            let file_size =
                usize::try_from(read_u64(data, base + 32)?).map_err(|_| ElfError::InvalidSegment)?;
            let memory_size = read_u64(data, base + 40)?;
            let alignment = read_u64(data, base + 48)?;

            if memory_size < file_size as u64
                || (alignment != 0 && !alignment.is_power_of_two())
                || (alignment > 1
                    && virtual_address % alignment != (offset as u64) % alignment)
            {
                return Err(ElfError::InvalidSegment);
            }

            let end = offset
                .checked_add(file_size)
                .ok_or(ElfError::TruncatedSegment)?;
            if end > data.len() {
                return Err(ElfError::TruncatedSegment);
            }

            segments.push(LoadSegment {
                virtual_address,
                memory_size,
                file_data: &data[offset..end],
                readable: flags & 4 != 0,
                writable: flags & 2 != 0,
                executable: flags & 1 != 0,
                alignment,
            });
        }

        if segments.is_empty() {
            return Err(ElfError::InvalidSegment);
        }

        Ok(Self { entry, segments })
    }

    pub fn entry(&self) -> u64 {
        self.entry
    }

    pub fn segments(&self) -> &[LoadSegment<'a>] {
        &self.segments
    }
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, ElfError> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or(ElfError::InvalidHeader)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, ElfError> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or(ElfError::InvalidHeader)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, ElfError> {
    let bytes = data
        .get(offset..offset + 8)
        .ok_or(ElfError::InvalidHeader)?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let mut elf = vec![0u8; 0x104];
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2;
        elf[5] = 1;
        elf[6] = 1;
        elf[16..18].copy_from_slice(&2u16.to_le_bytes());
        elf[18..20].copy_from_slice(&62u16.to_le_bytes());
        elf[20..24].copy_from_slice(&1u32.to_le_bytes());
        elf[24..32].copy_from_slice(&0x400000u64.to_le_bytes());
        elf[32..40].copy_from_slice(&64u64.to_le_bytes());
        elf[52..54].copy_from_slice(&64u16.to_le_bytes());
        elf[54..56].copy_from_slice(&56u16.to_le_bytes());
        elf[56..58].copy_from_slice(&1u16.to_le_bytes());

        let ph = 64;
        elf[ph..ph + 4].copy_from_slice(&1u32.to_le_bytes());
        elf[ph + 4..ph + 8].copy_from_slice(&5u32.to_le_bytes());
        elf[ph + 8..ph + 16].copy_from_slice(&0x100u64.to_le_bytes());
        elf[ph + 16..ph + 24].copy_from_slice(&0x400000u64.to_le_bytes());
        elf[ph + 32..ph + 40].copy_from_slice(&4u64.to_le_bytes());
        elf[ph + 40..ph + 48].copy_from_slice(&16u64.to_le_bytes());
        elf[ph + 48..ph + 56].copy_from_slice(&0x100u64.to_le_bytes());
        elf[0x100..0x104].copy_from_slice(&[0x90, 0x90, 0xcd, 0x80]);
        elf
    }

    #[test]
    fn parses_x86_64_load_segment() {
        let elf = fixture();
        let image = ElfImage::parse(&elf).unwrap();
        assert_eq!(image.entry(), 0x400000);
        assert_eq!(image.segments().len(), 1);
        let segment = image.segments()[0];
        assert_eq!(segment.virtual_address, 0x400000);
        assert_eq!(segment.memory_size, 16);
        assert_eq!(segment.file_data, &[0x90, 0x90, 0xcd, 0x80]);
        assert!(segment.readable);
        assert!(!segment.writable);
        assert!(segment.executable);
    }

    #[test]
    fn rejects_truncated_segment() {
        let mut elf = fixture();
        let ph = 64;
        elf[ph + 32..ph + 40].copy_from_slice(&128u64.to_le_bytes());
        assert_eq!(ElfImage::parse(&elf).unwrap_err(), ElfError::TruncatedSegment);
    }

    #[test]
    fn rejects_non_x86_64_and_bad_alignment() {
        let mut elf = fixture();
        elf[18..20].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(ElfImage::parse(&elf).unwrap_err(), ElfError::UnsupportedMachine);

        let mut elf = fixture();
        let ph = 64;
        elf[ph + 48..ph + 56].copy_from_slice(&3u64.to_le_bytes());
        assert_eq!(ElfImage::parse(&elf).unwrap_err(), ElfError::InvalidSegment);
    }
}
