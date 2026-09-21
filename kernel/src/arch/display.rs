use bootloader_api::info::{FrameBuffer, PixelFormat};
use spin::Mutex;

pub const ABI_VERSION: u32 = 1;
pub const SOURCE_FORMAT_XRGB8888: u32 = 1;

#[derive(Clone, Copy, Debug)]
pub struct DisplayInfo {
    pub width: usize,
    pub height: usize,
    pub native_pixel_format: u32,
}

#[derive(Clone, Copy)]
enum NativePixelFormat {
    Rgb,
    Bgr,
    U8,
    Other,
}

#[derive(Clone, Copy)]
struct DisplayState {
    address: usize,
    length: usize,
    width: usize,
    height: usize,
    stride: usize,
    bytes_per_pixel: usize,
    format: NativePixelFormat,
}

static DISPLAY: Mutex<Option<DisplayState>> = Mutex::new(None);

pub fn init(framebuffer: &mut FrameBuffer) {
    let info = framebuffer.info();
    let format = match info.pixel_format {
        PixelFormat::Rgb => NativePixelFormat::Rgb,
        PixelFormat::Bgr => NativePixelFormat::Bgr,
        PixelFormat::U8 => NativePixelFormat::U8,
        _ => NativePixelFormat::Other,
    };
    let buffer = framebuffer.buffer_mut();

    *DISPLAY.lock() = Some(DisplayState {
        address: buffer.as_mut_ptr() as usize,
        length: buffer.len(),
        width: info.width,
        height: info.height,
        stride: info.stride,
        bytes_per_pixel: info.bytes_per_pixel,
        format,
    });
}

pub fn is_ready() -> bool {
    DISPLAY.lock().is_some()
}

pub fn info() -> Option<DisplayInfo> {
    let state = (*DISPLAY.lock())?;
    Some(DisplayInfo {
        width: state.width,
        height: state.height,
        native_pixel_format: match state.format {
            NativePixelFormat::Rgb => 1,
            NativePixelFormat::Bgr => 2,
            NativePixelFormat::U8 => 3,
            NativePixelFormat::Other => 0,
        },
    })
}

pub fn present_xrgb8888(address: u64, length: usize) -> Result<(), &'static str> {
    let state = (*DISPLAY.lock()).ok_or("display is not initialized")?;
    let expected = state
        .width
        .checked_mul(state.height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("display size overflow")?;
    if length != expected {
        return Err("display buffer size mismatch");
    }

    crate::mm::validate_user_range(address, length, false)?;

    // SAFETY: the active process page tables were validated for the complete
    // source range above. The display state points at the boot framebuffer,
    // which remains mapped into every cloned supervisor address space.
    let source = unsafe { core::slice::from_raw_parts(address as *const u8, length) };
    let destination = state.address as *mut u8;

    for y in 0..state.height {
        for x in 0..state.width {
            let source_offset = (y * state.width + x) * 4;
            let blue = source[source_offset];
            let green = source[source_offset + 1];
            let red = source[source_offset + 2];

            let pixel = y
                .checked_mul(state.stride)
                .and_then(|row| row.checked_add(x))
                .ok_or("framebuffer pixel overflow")?;
            let destination_offset = pixel
                .checked_mul(state.bytes_per_pixel)
                .ok_or("framebuffer offset overflow")?;
            if destination_offset >= state.length {
                return Err("framebuffer mapping is too small");
            }

            let encoded = match state.format {
                NativePixelFormat::Rgb => [red, green, blue, 0],
                NativePixelFormat::Bgr => [blue, green, red, 0],
                NativePixelFormat::U8 => {
                    let intensity =
                        ((red as u16 + green as u16 + blue as u16) / 3) as u8;
                    [intensity, 0, 0, 0]
                }
                NativePixelFormat::Other => [blue, green, red, 0],
            };

            let count = state
                .bytes_per_pixel
                .min(encoded.len())
                .min(state.length - destination_offset);
            for (index, value) in encoded[..count].iter().copied().enumerate() {
                // SAFETY: destination_offset + index is bounded by state.length.
                unsafe {
                    core::ptr::write_volatile(destination.add(destination_offset + index), value);
                }
            }
        }
    }

    Ok(())
}
