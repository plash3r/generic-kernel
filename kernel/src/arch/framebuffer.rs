use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use core::fmt;

const GLYPH_WIDTH: usize = 5;
const GLYPH_HEIGHT: usize = 7;
const SCALE: usize = 2;
const CELL_WIDTH: usize = (GLYPH_WIDTH + 1) * SCALE;
const CELL_HEIGHT: usize = (GLYPH_HEIGHT + 1) * SCALE;
const DEFAULT_FG: Color = Color::new(0xe6, 0xed, 0xf3);
const ACCENT: Color = Color::new(0x55, 0xd1, 0x7a);

#[derive(Clone, Copy)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
}

impl Color {
    const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

pub struct Console<'a> {
    buffer: &'a mut [u8],
    info: FrameBufferInfo,
    column: usize,
    row: usize,
    columns: usize,
    rows: usize,
    foreground: Color,
}

impl<'a> Console<'a> {
    pub fn new(framebuffer: &'a mut FrameBuffer) -> Self {
        let info = framebuffer.info();
        let buffer = framebuffer.buffer_mut();
        let columns = (info.width / CELL_WIDTH).max(1);
        let rows = (info.height / CELL_HEIGHT).max(1);
        let mut console = Self {
            buffer,
            info,
            column: 0,
            row: 0,
            columns,
            rows,
            foreground: DEFAULT_FG,
        };
        console.clear();
        console
    }

    pub fn width(&self) -> usize {
        self.info.width
    }

    pub fn height(&self) -> usize {
        self.info.height
    }

    pub fn set_default_color(&mut self) {
        self.foreground = DEFAULT_FG;
    }

    pub fn set_accent_color(&mut self) {
        self.foreground = ACCENT;
    }

    pub fn clear(&mut self) {
        self.buffer.fill(0);
        self.column = 0;
        self.row = 0;
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b'\r' => self.column = 0,
            b'\t' => {
                for _ in 0..4 {
                    self.write_byte(b' ');
                }
            }
            0x20..=0x7e => {
                if self.column >= self.columns {
                    self.newline();
                }
                self.draw_glyph(byte, self.column * CELL_WIDTH, self.row * CELL_HEIGHT);
                self.column += 1;
            }
            _ => {}
        }
    }

    pub fn backspace(&mut self) {
        if self.column > 0 {
            self.column -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.column = self.columns.saturating_sub(1);
        } else {
            return;
        }
        self.clear_cell(self.column, self.row);
    }

    fn newline(&mut self) {
        self.column = 0;
        if self.row + 1 < self.rows {
            self.row += 1;
        } else {
            self.scroll();
        }
    }

    fn scroll(&mut self) {
        let bytes_per_row = self.info.stride.saturating_mul(self.info.bytes_per_pixel);
        let shift = CELL_HEIGHT.saturating_mul(bytes_per_row);
        let visible = self
            .info
            .height
            .saturating_mul(bytes_per_row)
            .min(self.buffer.len());

        if shift >= visible {
            self.clear();
            return;
        }

        self.buffer.copy_within(shift..visible, 0);
        self.buffer[visible - shift..visible].fill(0);
        self.row = self.rows.saturating_sub(1);
    }

    fn clear_cell(&mut self, column: usize, row: usize) {
        let start_x = column * CELL_WIDTH;
        let start_y = row * CELL_HEIGHT;
        for y in start_y..(start_y + CELL_HEIGHT).min(self.info.height) {
            for x in start_x..(start_x + CELL_WIDTH).min(self.info.width) {
                self.write_pixel(x, y, Color::new(0, 0, 0));
            }
        }
    }

    fn draw_glyph(&mut self, byte: u8, origin_x: usize, origin_y: usize) {
        let rows = glyph(byte);
        for (glyph_y, bits) in rows.iter().enumerate() {
            for glyph_x in 0..GLYPH_WIDTH {
                if bits & (1 << (GLYPH_WIDTH - 1 - glyph_x)) == 0 {
                    continue;
                }
                for scale_y in 0..SCALE {
                    for scale_x in 0..SCALE {
                        self.write_pixel(
                            origin_x + glyph_x * SCALE + scale_x,
                            origin_y + glyph_y * SCALE + scale_y,
                            self.foreground,
                        );
                    }
                }
            }
        }
    }

    fn write_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }

        let pixel = y.saturating_mul(self.info.stride).saturating_add(x);
        let offset = pixel.saturating_mul(self.info.bytes_per_pixel);
        if offset >= self.buffer.len() {
            return;
        }

        let encoded = match self.info.pixel_format {
            PixelFormat::Rgb => [color.r, color.g, color.b, 0],
            PixelFormat::Bgr => [color.b, color.g, color.r, 0],
            PixelFormat::U8 => {
                let intensity =
                    ((color.r as u16 + color.g as u16 + color.b as u16) / 3) as u8;
                [intensity, 0, 0, 0]
            }
            _ => [color.r, color.g, color.b, 0],
        };

        let count = self
            .info
            .bytes_per_pixel
            .min(encoded.len())
            .min(self.buffer.len() - offset);
        for (index, value) in encoded[..count].iter().copied().enumerate() {
            // SAFETY: the bounds above guarantee that offset + index is inside
            // the mapped framebuffer. Volatile writes are appropriate for MMIO.
            unsafe {
                core::ptr::write_volatile(self.buffer.as_mut_ptr().add(offset + index), value);
            }
        }
    }
}

impl fmt::Write for Console<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            self.write_byte(byte);
        }
        Ok(())
    }
}

fn glyph(byte: u8) -> [u8; GLYPH_HEIGHT] {
    match byte.to_ascii_uppercase() {
        b' ' => [0, 0, 0, 0, 0, 0, 0],
        b'A' => [14, 17, 17, 31, 17, 17, 17],
        b'B' => [30, 17, 17, 30, 17, 17, 30],
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'D' => [30, 17, 17, 17, 17, 17, 30],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'F' => [31, 16, 16, 30, 16, 16, 16],
        b'G' => [14, 17, 16, 23, 17, 17, 15],
        b'H' => [17, 17, 17, 31, 17, 17, 17],
        b'I' => [14, 4, 4, 4, 4, 4, 14],
        b'J' => [1, 1, 1, 1, 17, 17, 14],
        b'K' => [17, 18, 20, 24, 20, 18, 17],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'M' => [17, 27, 21, 21, 17, 17, 17],
        b'N' => [17, 25, 21, 19, 17, 17, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'P' => [30, 17, 17, 30, 16, 16, 16],
        b'Q' => [14, 17, 17, 17, 21, 18, 13],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'U' => [17, 17, 17, 17, 17, 17, 14],
        b'V' => [17, 17, 17, 17, 17, 10, 4],
        b'W' => [17, 17, 17, 21, 21, 21, 10],
        b'X' => [17, 17, 10, 4, 10, 17, 17],
        b'Y' => [17, 17, 10, 4, 4, 4, 4],
        b'Z' => [31, 1, 2, 4, 8, 16, 31],
        b'0' => [14, 17, 19, 21, 25, 17, 14],
        b'1' => [4, 12, 4, 4, 4, 4, 14],
        b'2' => [14, 17, 1, 2, 4, 8, 31],
        b'3' => [30, 1, 1, 14, 1, 1, 30],
        b'4' => [2, 6, 10, 18, 31, 2, 2],
        b'5' => [31, 16, 16, 30, 1, 1, 30],
        b'6' => [14, 16, 16, 30, 17, 17, 14],
        b'7' => [31, 1, 2, 4, 8, 8, 8],
        b'8' => [14, 17, 17, 14, 17, 17, 14],
        b'9' => [14, 17, 17, 15, 1, 1, 14],
        b'>' => [16, 8, 4, 2, 4, 8, 16],
        b'<' => [1, 2, 4, 8, 4, 2, 1],
        b':' => [0, 4, 0, 0, 4, 0, 0],
        b';' => [0, 4, 0, 0, 4, 4, 8],
        b'.' => [0, 0, 0, 0, 0, 4, 4],
        b',' => [0, 0, 0, 0, 4, 4, 8],
        b'-' => [0, 0, 0, 31, 0, 0, 0],
        b'_' => [0, 0, 0, 0, 0, 0, 31],
        b'/' => [1, 2, 2, 4, 8, 8, 16],
        b'\\' => [16, 8, 8, 4, 2, 2, 1],
        b'[' => [14, 8, 8, 8, 8, 8, 14],
        b']' => [14, 2, 2, 2, 2, 2, 14],
        b'(' => [2, 4, 8, 8, 8, 4, 2],
        b')' => [8, 4, 2, 2, 2, 4, 8],
        b'=' => [0, 31, 0, 31, 0, 0, 0],
        b'+' => [0, 4, 4, 31, 4, 4, 0],
        b'*' => [0, 17, 10, 31, 10, 17, 0],
        b'!' => [4, 4, 4, 4, 4, 0, 4],
        b'?' => [14, 17, 1, 2, 4, 0, 4],
        b'\'' => [4, 4, 2, 0, 0, 0, 0],
        b'"' => [10, 10, 5, 0, 0, 0, 0],
        b'#' => [10, 31, 10, 10, 31, 10, 0],
        b'$' => [4, 15, 20, 14, 5, 30, 4],
        b'%' => [24, 25, 2, 4, 8, 19, 3],
        b'@' => [14, 17, 23, 21, 23, 16, 14],
        b'&' => [12, 18, 20, 8, 21, 18, 13],
        b'|' => [4, 4, 4, 4, 4, 4, 4],
        b'^' => [4, 10, 17, 0, 0, 0, 0],
        0x60 => [8, 4, 2, 0, 0, 0, 0],
        _ => [31, 17, 1, 2, 4, 0, 4],
    }
}
