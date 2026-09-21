use alloc::vec::Vec;
use core::{fmt, str};

const PSF2_MAGIC: u32 = 0x864a_b572;
const PSF2_HEADER_SIZE: usize = 32;
const PSF2_HAS_UNICODE_TABLE: u32 = 1;
const MAX_DIMENSION: usize = 64;
const MAX_GLYPHS: usize = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsfError {
    TooSmall,
    BadMagic,
    UnsupportedVersion,
    InvalidHeader,
    InvalidDimensions,
    InvalidGlyphCount,
    InvalidCharSize,
    TruncatedGlyphData,
}

impl fmt::Display for PsfError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooSmall => "PSF2 file is too small",
            Self::BadMagic => "not a PSF2 font",
            Self::UnsupportedVersion => "unsupported PSF2 version",
            Self::InvalidHeader => "invalid PSF2 header",
            Self::InvalidDimensions => "invalid PSF2 glyph dimensions",
            Self::InvalidGlyphCount => "invalid PSF2 glyph count",
            Self::InvalidCharSize => "invalid PSF2 glyph size",
            Self::TruncatedGlyphData => "truncated PSF2 glyph data",
        })
    }
}

/// Validated PSF2 bitmap font.
///
/// The complete file is kept owned so the renderer can load fonts directly
/// from Generic VFS without leaking buffers or requiring a static lifetime.
#[derive(Debug)]
pub struct Psf2Font {
    data: Vec<u8>,
    header_size: usize,
    glyph_count: usize,
    char_size: usize,
    height: usize,
    width: usize,
    bytes_per_row: usize,
    flags: u32,
    glyph_data_end: usize,
}

impl Psf2Font {
    pub fn parse(data: Vec<u8>) -> Result<Self, PsfError> {
        if data.len() < PSF2_HEADER_SIZE {
            return Err(PsfError::TooSmall);
        }
        if read_u32(&data, 0) != PSF2_MAGIC {
            return Err(PsfError::BadMagic);
        }
        if read_u32(&data, 4) != 0 {
            return Err(PsfError::UnsupportedVersion);
        }

        let header_size = read_u32(&data, 8) as usize;
        let flags = read_u32(&data, 12);
        let glyph_count = read_u32(&data, 16) as usize;
        let char_size = read_u32(&data, 20) as usize;
        let height = read_u32(&data, 24) as usize;
        let width = read_u32(&data, 28) as usize;

        if header_size < PSF2_HEADER_SIZE || header_size > data.len() {
            return Err(PsfError::InvalidHeader);
        }
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err(PsfError::InvalidDimensions);
        }
        if glyph_count == 0 || glyph_count > MAX_GLYPHS {
            return Err(PsfError::InvalidGlyphCount);
        }

        let bytes_per_row = width.div_ceil(8);
        let minimum_char_size = bytes_per_row
            .checked_mul(height)
            .ok_or(PsfError::InvalidCharSize)?;
        if char_size < minimum_char_size {
            return Err(PsfError::InvalidCharSize);
        }

        let glyph_bytes = glyph_count
            .checked_mul(char_size)
            .ok_or(PsfError::TruncatedGlyphData)?;
        let glyph_data_end = header_size
            .checked_add(glyph_bytes)
            .ok_or(PsfError::TruncatedGlyphData)?;
        if glyph_data_end > data.len() {
            return Err(PsfError::TruncatedGlyphData);
        }

        Ok(Self {
            data,
            header_size,
            glyph_count,
            char_size,
            height,
            width,
            bytes_per_row,
            flags,
            glyph_data_end,
        })
    }

    pub const fn width(&self) -> usize {
        self.width
    }

    pub const fn height(&self) -> usize {
        self.height
    }

    pub const fn glyph_count(&self) -> usize {
        self.glyph_count
    }

    pub fn pixel(&self, character: char, x: usize, y: usize) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }

        let glyph = self.glyph_index(character);
        let row = self.header_size + glyph * self.char_size + y * self.bytes_per_row;
        let byte = self.data[row + x / 8];
        byte & (0x80 >> (x % 8)) != 0
    }

    fn glyph_index(&self, character: char) -> usize {
        if self.flags & PSF2_HAS_UNICODE_TABLE != 0 {
            if let Some(index) = self.unicode_glyph_index(character) {
                return index;
            }
        }

        let direct = character as usize;
        if direct < self.glyph_count {
            direct
        } else if ('?' as usize) < self.glyph_count {
            '?' as usize
        } else {
            0
        }
    }

    fn unicode_glyph_index(&self, character: char) -> Option<usize> {
        let table = self.data.get(self.glyph_data_end..)?;
        let mut glyph = 0usize;
        let mut position = 0usize;

        while glyph < self.glyph_count && position < table.len() {
            if table[position] == 0xff {
                glyph += 1;
                position += 1;
                continue;
            }
            if table[position] == 0xfe {
                position += 1;
                continue;
            }

            let width = utf8_sequence_len(table[position])?;
            let end = position.checked_add(width)?;
            let bytes = table.get(position..end)?;
            if let Ok(text) = str::from_utf8(bytes) {
                if text.chars().next() == Some(character) {
                    return Some(glyph);
                }
            }
            position = end;
        }

        None
    }
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn utf8_sequence_len(first: u8) -> Option<usize> {
    match first {
        0x00..=0x7f => Some(1),
        0xc2..=0xdf => Some(2),
        0xe0..=0xef => Some(3),
        0xf0..=0xf4 => Some(4),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn font_with_glyphs(width: u32, height: u32, glyph_count: u32) -> Vec<u8> {
        let bytes_per_row = (width as usize).div_ceil(8);
        let char_size = bytes_per_row * height as usize;
        let mut data = vec![0u8; PSF2_HEADER_SIZE + char_size * glyph_count as usize];
        data[0..4].copy_from_slice(&PSF2_MAGIC.to_le_bytes());
        data[4..8].copy_from_slice(&0u32.to_le_bytes());
        data[8..12].copy_from_slice(&(PSF2_HEADER_SIZE as u32).to_le_bytes());
        data[12..16].copy_from_slice(&0u32.to_le_bytes());
        data[16..20].copy_from_slice(&glyph_count.to_le_bytes());
        data[20..24].copy_from_slice(&(char_size as u32).to_le_bytes());
        data[24..28].copy_from_slice(&height.to_le_bytes());
        data[28..32].copy_from_slice(&width.to_le_bytes());
        data
    }

    #[test]
    fn parses_and_reads_direct_ascii_glyph() {
        let mut data = font_with_glyphs(8, 8, 128);
        let glyph_size = 8usize;
        let a = PSF2_HEADER_SIZE + ('A' as usize) * glyph_size;
        data[a] = 0b1000_0001;

        let font = Psf2Font::parse(data).unwrap();
        assert_eq!(font.width(), 8);
        assert_eq!(font.height(), 8);
        assert_eq!(font.glyph_count(), 128);
        assert!(font.pixel('A', 0, 0));
        assert!(font.pixel('A', 7, 0));
        assert!(!font.pixel('A', 1, 0));
    }

    #[test]
    fn rejects_invalid_dimensions_and_truncated_data() {
        assert_eq!(
            Psf2Font::parse(font_with_glyphs(0, 8, 128)).unwrap_err(),
            PsfError::InvalidDimensions
        );

        let mut data = font_with_glyphs(8, 16, 128);
        data.truncate(data.len() - 1);
        assert_eq!(
            Psf2Font::parse(data).unwrap_err(),
            PsfError::TruncatedGlyphData
        );
    }

    #[test]
    fn unicode_table_can_override_direct_glyph_index() {
        let mut data = font_with_glyphs(8, 8, 2);
        data[12..16].copy_from_slice(&PSF2_HAS_UNICODE_TABLE.to_le_bytes());
        let glyph_size = 8usize;
        let glyph_one = PSF2_HEADER_SIZE + glyph_size;
        data[glyph_one] = 0x80;
        data.extend_from_slice(&[b'A', 0xff, b'B', 0xff]);

        let font = Psf2Font::parse(data).unwrap();
        assert!(font.pixel('B', 0, 0));
        assert!(!font.pixel('A', 0, 0));
    }
}
