use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use core::fmt;
use noto_sans_mono_bitmap::{
    get_raster, get_raster_width, FontWeight, RasterHeight, RasterizedChar,
};

const FONT_WEIGHT: FontWeight = FontWeight::Regular;
const FONT_HEIGHT: RasterHeight = RasterHeight::Size16;
const GLYPH_WIDTH: usize = get_raster_width(FONT_WEIGHT, FONT_HEIGHT);
const GLYPH_HEIGHT: usize = FONT_HEIGHT.val();
const CELL_PADDING_X: usize = 1;
const CELL_PADDING_Y: usize = 1;
const CELL_WIDTH: usize = GLYPH_WIDTH + CELL_PADDING_X * 2;
const CELL_HEIGHT: usize = GLYPH_HEIGHT + CELL_PADDING_Y * 2;

const BACKGROUND: Color = Color::new(0x00, 0x00, 0x00);
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

    fn with_intensity(self, intensity: u8) -> Self {
        let scale = intensity as u16;
        Self {
            r: ((self.r as u16 * scale) / 255) as u8,
            g: ((self.g as u16 * scale) / 255) as u8,
            b: ((self.b as u16 * scale) / 255) as u8,
        }
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
        self.fill_background();
        self.column = 0;
        self.row = 0;
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b'\r' => self.column = 0,
            b'\t' => self.write_tab(),
            0x20..=0x7e => self.write_character(byte as char),
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

    fn write_character(&mut self, character: char) {
        match character {
            '\n' => self.newline(),
            '\r' => self.column = 0,
            '\t' => self.write_tab(),
            character if !character.is_control() => {
                if self.column >= self.columns {
                    self.newline();
                }
                self.draw_glyph(character, self.column * CELL_WIDTH, self.row * CELL_HEIGHT);
                self.column += 1;
            }
            _ => {}
        }
    }

    fn write_tab(&mut self) {
        let spaces = 4 - (self.column % 4);
        for _ in 0..spaces {
            self.write_character(' ');
        }
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

    fn fill_background(&mut self) {
        if BACKGROUND.r == 0 && BACKGROUND.g == 0 && BACKGROUND.b == 0 {
            self.buffer.fill(0);
            return;
        }

        for y in 0..self.info.height {
            for x in 0..self.info.width {
                self.write_pixel(x, y, BACKGROUND);
            }
        }
    }

    fn clear_cell(&mut self, column: usize, row: usize) {
        let start_x = column * CELL_WIDTH;
        let start_y = row * CELL_HEIGHT;
        for y in start_y..(start_y + CELL_HEIGHT).min(self.info.height) {
            for x in start_x..(start_x + CELL_WIDTH).min(self.info.width) {
                self.write_pixel(x, y, BACKGROUND);
            }
        }
    }

    fn draw_glyph(&mut self, character: char, origin_x: usize, origin_y: usize) {
        let raster = glyph(character);
        debug_assert_eq!(raster.width(), GLYPH_WIDTH);
        debug_assert_eq!(raster.height(), GLYPH_HEIGHT);

        for (glyph_y, row) in raster.raster().iter().enumerate() {
            for (glyph_x, intensity) in row.iter().copied().enumerate() {
                if intensity == 0 {
                    continue;
                }
                self.write_pixel(
                    origin_x + CELL_PADDING_X + glyph_x,
                    origin_y + CELL_PADDING_Y + glyph_y,
                    self.foreground.with_intensity(intensity),
                );
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
                let intensity = ((color.r as u16 + color.g as u16 + color.b as u16) / 3) as u8;
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
        for character in text.chars() {
            self.write_character(character);
        }
        Ok(())
    }
}

fn glyph(character: char) -> RasterizedChar {
    get_raster(character, FONT_WEIGHT, FONT_HEIGHT)
        .or_else(|| get_raster('?', FONT_WEIGHT, FONT_HEIGHT))
        .expect("Noto Sans Mono basic Latin must contain '?'")
}
