use alloc::{vec, vec::Vec};
use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use noto_sans_mono_bitmap::{
    get_raster, get_raster_width, FontWeight, RasterHeight,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(self, point: Point) -> bool {
        point.x >= self.x
            && point.y >= self.y
            && point.x < self.x.saturating_add(self.width)
            && point.y < self.y.saturating_add(self.height)
    }

    fn intersect(self, other: Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self
            .x
            .saturating_add(self.width)
            .min(other.x.saturating_add(other.width));
        let bottom = self
            .y
            .saturating_add(self.height)
            .min(other.y.saturating_add(other.height));
        if right <= left || bottom <= top {
            None
        } else {
            Some(Self::new(left, top, right - left, bottom - top))
        }
    }

    fn union(self, other: Self) -> Self {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = self
            .x
            .saturating_add(self.width)
            .max(other.x.saturating_add(other.width));
        let bottom = self
            .y
            .saturating_add(self.height)
            .max(other.y.saturating_add(other.height));
        Self::new(left, top, right - left, bottom - top)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(u32);

impl Color {
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(((r as u32) << 16) | ((g as u32) << 8) | b as u32)
    }

    pub const fn r(self) -> u8 {
        (self.0 >> 16) as u8
    }

    pub const fn g(self) -> u8 {
        (self.0 >> 8) as u8
    }

    pub const fn b(self) -> u8 {
        self.0 as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextStyle {
    Regular16,
    Bold16,
    Bold20,
}

impl TextStyle {
    fn weight(self) -> FontWeight {
        match self {
            Self::Regular16 => FontWeight::Regular,
            Self::Bold16 | Self::Bold20 => FontWeight::Bold,
        }
    }

    fn height(self) -> RasterHeight {
        match self {
            Self::Regular16 | Self::Bold16 => RasterHeight::Size16,
            Self::Bold20 => RasterHeight::Size20,
        }
    }

    pub fn line_height(self) -> i32 {
        self.height().val() as i32 + 3
    }

    pub fn glyph_width(self) -> i32 {
        get_raster_width(self.weight(), self.height()) as i32
    }
}

pub struct Display<'a> {
    front: &'a mut [u8],
    info: FrameBufferInfo,
    back: Vec<u32>,
    dirty: Option<Rect>,
}

impl<'a> Display<'a> {
    pub fn new(framebuffer: &'a mut FrameBuffer) -> Self {
        let info = framebuffer.info();
        Self::from_buffer(framebuffer.buffer_mut(), info)
    }

    pub fn from_buffer(front: &'a mut [u8], info: FrameBufferInfo) -> Self {
        let pixels = info
            .width
            .checked_mul(info.height)
            .expect("framebuffer dimensions overflow");
        let back = vec![0; pixels];
        Self {
            front,
            info,
            back,
            dirty: Some(Rect::new(0, 0, info.width as i32, info.height as i32)),
        }
    }

    pub fn width(&self) -> i32 {
        self.info.width as i32
    }

    pub fn height(&self) -> i32 {
        self.info.height as i32
    }

    pub fn backbuffer_bytes(&self) -> usize {
        self.back.len() * core::mem::size_of::<u32>()
    }

    pub fn clear(&mut self, color: Color) {
        self.back.fill(color.0);
        self.dirty = Some(self.screen_rect());
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        let Some(rect) = rect.intersect(self.screen_rect()) else {
            return;
        };
        for y in rect.y..rect.y + rect.height {
            let start = y as usize * self.info.width + rect.x as usize;
            let end = start + rect.width as usize;
            self.back[start..end].fill(color.0);
        }
        self.mark_dirty(rect);
    }

    pub fn stroke_rect(&mut self, rect: Rect, color: Color, thickness: i32) {
        if thickness <= 0 || rect.width <= 0 || rect.height <= 0 {
            return;
        }
        self.fill_rect(Rect::new(rect.x, rect.y, rect.width, thickness), color);
        self.fill_rect(
            Rect::new(
                rect.x,
                rect.y + rect.height - thickness,
                rect.width,
                thickness,
            ),
            color,
        );
        self.fill_rect(Rect::new(rect.x, rect.y, thickness, rect.height), color);
        self.fill_rect(
            Rect::new(
                rect.x + rect.width - thickness,
                rect.y,
                thickness,
                rect.height,
            ),
            color,
        );
    }

    pub fn line(&mut self, from: Point, to: Point, color: Color) {
        let mut x0 = from.x;
        let mut y0 = from.y;
        let x1 = to.x;
        let y1 = to.y;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;

        loop {
            self.pixel(x0, y0, color);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let doubled = error.saturating_mul(2);
            if doubled >= dy {
                error += dy;
                x0 += sx;
            }
            if doubled <= dx {
                error += dx;
                y0 += sy;
            }
        }
    }

    pub fn text(
        &mut self,
        mut x: i32,
        mut y: i32,
        text: &str,
        color: Color,
        style: TextStyle,
    ) {
        let start_x = x;
        let weight = style.weight();
        let height = style.height();
        let glyph_width = get_raster_width(weight, height) as i32;
        let line_height = style.line_height();

        for character in text.chars() {
            if character == '\n' {
                x = start_x;
                y += line_height;
                continue;
            }

            let raster = get_raster(character, weight, height)
                .or_else(|| get_raster('?', weight, height));
            let Some(raster) = raster else {
                x += glyph_width;
                continue;
            };

            for (row_index, row) in raster.raster().iter().enumerate() {
                for (column_index, intensity) in row.iter().copied().enumerate() {
                    if intensity == 0 {
                        continue;
                    }
                    self.blend_pixel(
                        x + column_index as i32,
                        y + row_index as i32,
                        color,
                        intensity,
                    );
                }
            }
            x += glyph_width;
        }
    }

    pub fn present(&mut self) {
        let Some(dirty) = self.dirty.take() else {
            return;
        };
        let Some(dirty) = dirty.intersect(self.screen_rect()) else {
            return;
        };

        for y in dirty.y..dirty.y + dirty.height {
            for x in dirty.x..dirty.x + dirty.width {
                let color = Color(self.back[y as usize * self.info.width + x as usize]);
                self.write_front_pixel(x as usize, y as usize, color);
            }
        }
    }

    pub fn checksum(&self) -> u64 {
        self.back.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, pixel| {
            hash.wrapping_mul(0x100_0000_01b3) ^ *pixel as u64
        })
    }

    fn pixel(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || y < 0 || x >= self.width() || y >= self.height() {
            return;
        }
        self.back[y as usize * self.info.width + x as usize] = color.0;
        self.mark_dirty(Rect::new(x, y, 1, 1));
    }

    fn blend_pixel(&mut self, x: i32, y: i32, source: Color, alpha: u8) {
        if x < 0 || y < 0 || x >= self.width() || y >= self.height() {
            return;
        }
        let index = y as usize * self.info.width + x as usize;
        let destination = Color(self.back[index]);
        let alpha = alpha as u32;
        let inverse = 255 - alpha;
        let blend = |source: u8, destination: u8| -> u8 {
            ((source as u32 * alpha + destination as u32 * inverse) / 255) as u8
        };
        self.back[index] = Color::rgb(
            blend(source.r(), destination.r()),
            blend(source.g(), destination.g()),
            blend(source.b(), destination.b()),
        )
        .0;
        self.mark_dirty(Rect::new(x, y, 1, 1));
    }

    fn write_front_pixel(&mut self, x: usize, y: usize, color: Color) {
        let pixel = y.saturating_mul(self.info.stride).saturating_add(x);
        let offset = pixel.saturating_mul(self.info.bytes_per_pixel);
        if offset >= self.front.len() {
            return;
        }

        let mut native = match self.info.pixel_format {
            PixelFormat::Rgb => {
                color.r() as u32 | ((color.g() as u32) << 8) | ((color.b() as u32) << 16)
            }
            PixelFormat::Bgr => {
                color.b() as u32 | ((color.g() as u32) << 8) | ((color.r() as u32) << 16)
            }
            PixelFormat::U8 => {
                ((color.r() as u32 + color.g() as u32 + color.b() as u32) / 3) & 0xff
            }
            PixelFormat::Unknown {
                red_position,
                green_position,
                blue_position,
            } => {
                ((color.r() as u32) << red_position)
                    | ((color.g() as u32) << green_position)
                    | ((color.b() as u32) << blue_position)
            }
            _ => color.0,
        };

        let count = self
            .info
            .bytes_per_pixel
            .min(core::mem::size_of::<u32>())
            .min(self.front.len() - offset);
        for byte in 0..count {
            let value = native as u8;
            // SAFETY: offset + byte is bounds-checked above. Volatile writes are
            // used because the destination is a hardware framebuffer mapping.
            unsafe {
                core::ptr::write_volatile(self.front.as_mut_ptr().add(offset + byte), value);
            }
            native >>= 8;
        }
    }

    fn screen_rect(&self) -> Rect {
        Rect::new(0, 0, self.width(), self.height())
    }

    fn mark_dirty(&mut self, rect: Rect) {
        self.dirty = Some(match self.dirty {
            Some(current) => current.union(rect),
            None => rect,
        });
    }
}

pub fn smoke(framebuffer: &mut FrameBuffer) {
    let mut display = Display::new(framebuffer);
    display.clear(Color::rgb(18, 28, 48));
    let width = display.width();
    let height = display.height();
    display.fill_rect(
        Rect::new(24, 24, (width - 48).max(1), (height - 48).max(1)),
        Color::rgb(28, 43, 70),
    );
    display.stroke_rect(
        Rect::new(24, 24, (width - 48).max(1), (height - 48).max(1)),
        Color::rgb(83, 177, 255),
        2,
    );
    display.text(
        48,
        48,
        "Generic graphics backbuffer",
        Color::WHITE,
        TextStyle::Bold20,
    );
    display.line(
        Point { x: 48, y: 84 },
        Point {
            x: (width - 48).max(48),
            y: 84,
        },
        Color::rgb(83, 177, 255),
    );
    let checksum = display.checksum();
    let bytes = display.backbuffer_bytes();
    display.present();
    crate::log!(
        "[ok] graphics backbuffer {}x{}, {} KiB, checksum={:#x}\n",
        width,
        height,
        bytes / 1024,
        checksum
    );
}
