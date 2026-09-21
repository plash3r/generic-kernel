use core::fmt::{self, Write};
use spin::Mutex;
use x86_64::instructions::{interrupts, port::Port};

static SERIAL: Mutex<Serial> = Mutex::new(Serial);
struct Serial;

impl Serial {
    fn write_byte(&mut self, byte: u8) {
        // Bounded polling: absent/broken UART must not hang a panic forever.
        for _ in 0..100_000 {
            // SAFETY: reading COM1 line status has no memory side effects.
            if unsafe { Port::<u8>::new(0x3fd).read() } & 0x20 != 0 {
                // SAFETY: the caller holds the UART lock.
                unsafe {
                    Port::<u8>::new(0x3f8).write(byte);
                }
                break;
            }
            core::hint::spin_loop();
        }
    }
}

pub fn init() {
    // SAFETY: bootstrap CPU owns COM1, with interrupts disabled.
    unsafe {
        Port::<u8>::new(0x3f9).write(0);
        Port::<u8>::new(0x3fb).write(0x80);
        Port::<u8>::new(0x3f8).write(3);
        Port::<u8>::new(0x3f9).write(0);
        Port::<u8>::new(0x3fb).write(3);
        Port::<u8>::new(0x3fa).write(0xc7);
        Port::<u8>::new(0x3fc).write(0x0b);
    }
}

impl Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
        Ok(())
    }
}

pub fn write_bytes(bytes: &[u8]) {
    interrupts::without_interrupts(|| {
        if let Some(mut serial) = SERIAL.try_lock() {
            for &byte in bytes {
                serial.write_byte(byte);
            }
        }
    });
}

pub fn print(args: fmt::Arguments<'_>) {
    interrupts::without_interrupts(|| {
        // Exceptions/panics may interrupt a writer; drop recursive output instead of deadlocking.
        if let Some(mut serial) = SERIAL.try_lock() {
            let _ = serial.write_fmt(args);
        }
    });
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::arch::serial::print(format_args!($($arg)*)) };
}
