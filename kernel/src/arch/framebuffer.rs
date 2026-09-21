use alloc::vec::Vec;
use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use core::fmt;
use kernel_core::psf::{Psf2Font, PsfError};
use noto_sans_mono_bitmap::{
    get_raster, get_raster_width, FontWeight, RasterHeight, RasterizedChar,
};

const CELL_PADDING_X: usize = 1;
const CELL_PADDING_Y: usize = 1;

const BACKGROUND: Color = Color::new(0x00, 0x00, 0x00);
const DEFAULT_FG: Color = Color::new(0xe6, 0xed, 0xf3);
const ACCENT: Color = Color::new(0x55, 0xd1, 0x7a);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontPreset {
    Noto16,
    Noto20,
    Noto24,
    Bold16,
    Bold20,
}

impl FontPreset {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Noto16 => "noto16",
            Self::Noto20 => "noto20",
            Self::Noto24 => "noto24",
            Self::Bold16 => "bold16",
            Self::Bold20 => "bold20",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("noto16") || name == "16" {
            Some(Self::Noto16)
        } else if name.eq_ignore_ascii_case("noto20") || name == "20" {
            Some(Self::Noto20)
        } else if name.eq_ignore_ascii_case("noto24") || name == "24" {
            Some(Self::Noto24)
        } else if name.eq_ignore_ascii_case("bold16") {
            Some(Self::Bold16)
        } else if name.eq_ignore_ascii_case("bold20") {
            Some(Self::Bold20)
        } else {
            None
        }
    }

    const fn weight(self) -> FontWeight {
        match self {
            Self::Noto16 | Self::Noto20 | Self::Noto24 => FontWeight::Regular,
            Self::Bold16 | Self::Bold20 => FontWeight::Bold,
        }
    }

    const fn height(self) -> RasterHeight {
        match self {
            Self::Noto16 | Self::Bold16 => RasterHeight::Size16,
            Self::Noto20 | Self::Bold20 => RasterHeight::Size20,
            Self::Noto24 => RasterHeight::Size24,
        }
    }

    fn glyph_width(self) -> usize {
        get_raster_width(self.weight(), self.height())
    }

    const fn glyph_height(self) -> usize {
        self.height().val()
    }

    fn raster(self, character: char) -> RasterizedChar {
        get_raster(character, self.weight(), self.height())
            .or_else(|| get_raster('?', self.weight(), self.height()))
            .expect("Noto Sans Mono basic Latin must contain '?'")
    }
}

enum FontSelection {
    Builtin(FontPreset),
    Psf2(Psf2Font),
}

impl FontSelection {
    fn glyph_width(&self) -> usize {
        match self {
            Self::Builtin(preset) => preset.glyph_width(),
            Self::Psf2(font) => font.width(),
        }
    }

    fn glyph_height(&self) -> usize {
        match self {
            Self::Builtin(preset) => preset.glyph_height(),
            Self::Psf2(font) => font.height(),
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Builtin(preset) => preset.name(),
            Self::Psf2(_) => "custom-psf2",
        }
    }
}

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
    font: FontSelection,
}

impl<'a> Console<'a> {
    pub fn new(framebuffer: &'a mut FrameBuffer) -> Self {
        let info = framebuffer.info();
        let buffer = framebuffer.buffer_mut();
        let mut console = Self {
            buffer,
            info,
            column: 0,
            row: 0,
            columns: 1,
            rows: 1,
            foreground: DEFAULT_FG,
            font: FontSelection::Builtin(FontPreset::Noto16),
        };
        console.recalculate_grid();
        console.clear();
        console
    }

    pub fn width(&self) -> usize {
        self.info.width
    }

    pub fn height(&self) -> usize {
        self.info.height
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn font_name(&self) -> &'static str {
        self.font.name()
    }

    pub fn font_dimensions(&self) -> (usize, usize) {
        (self.font.glyph_width(), self.font.glyph_height())
    }

    pub fn set_font_preset(&mut self, preset: FontPreset) {
        self.font = FontSelection::Builtin(preset);
        self.recalculate_grid();
        self.clear();
    }

    pub fn load_psf2(&mut self, data: Vec<u8>) -> Result<usize, PsfError> {
        let font = Psf2Font::parse(data)?;
        let glyphs = font.glyph_count();
        self.font = FontSelection::Psf2(font);
        self.recalculate_grid();
        self.clear();
        Ok(glyphs)
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

    fn cell_width(&self) -> usize {
        self.font.glyph_width() + CELL_PADDING_X * 2
    }

    fn cell_height(&self) -> usize {
        self.font.glyph_height() + CELL_PADDING_Y * 2
    }

    fn recalculate_grid(&mut self) {
        self.columns = (self.info.width / self.cell_width()).max(1);
        self.rows = (self.info.height / self.cell_height()).max(1);
        self.column = 0;
        self.row = 0;
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
                let x = self.column * self.cell_width();
                let y = self.row * self.cell_height();
                self.draw_glyph(character, x, y);
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
        let shift = self.cell_height().saturating_mul(bytes_per_row);
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
        let cell_width = self.cell_width();
        let cell_height = self.cell_height();
        let start_x = column * cell_width;
        let start_y = row * cell_height;
        for y in start_y..(start_y + cell_height).min(self.info.height) {
            for x in start_x..(start_x + cell_width).min(self.info.width) {
                self.write_pixel(x, y, BACKGROUND);
            }
        }
    }

    fn draw_glyph(&mut self, character: char, origin_x: usize, origin_y: usize) {
        if let FontSelection::Builtin(preset) = &self.font {
            let preset = *preset;
            let raster = preset.raster(character);
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
            return;
        }

        let width = self.font.glyph_width();
        let height = self.font.glyph_height();
        for glyph_y in 0..height {
            for glyph_x in 0..width {
                let enabled = match &self.font {
                    FontSelection::Psf2(font) => font.pixel(character, glyph_x, glyph_y),
                    FontSelection::Builtin(_) => false,
                };
                if enabled {
                    self.write_pixel(
                        origin_x + CELL_PADDING_X + glyph_x,
                        origin_y + CELL_PADDING_Y + glyph_y,
                        self.foreground,
                    );
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
