use spin::Mutex;

use crate::arch::keyboard::{Key, Keyboard};

pub const PACKET_BYTES: usize = 24;
pub const INPUT_KEY: u32 = 1;
pub const INPUT_MOUSE: u32 = 2;

pub const KEY_TAB: u32 = 0x100;
pub const KEY_ENTER: u32 = 0x101;
pub const KEY_ESCAPE: u32 = 0x102;
pub const KEY_BACKSPACE: u32 = 0x103;
pub const KEY_F12: u32 = 0x104;
pub const KEY_ARROW_UP: u32 = 0x110;
pub const KEY_ARROW_DOWN: u32 = 0x111;
pub const KEY_ARROW_LEFT: u32 = 0x112;
pub const KEY_ARROW_RIGHT: u32 = 0x113;

#[derive(Clone, Copy, Debug, Default)]
pub struct InputPacket {
    pub kind: u32,
    pub code: u32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub flags: u32,
}

impl InputPacket {
    pub fn encode(self) -> [u8; PACKET_BYTES] {
        let mut bytes = [0u8; PACKET_BYTES];
        write_u32(&mut bytes, 0, self.kind);
        write_u32(&mut bytes, 4, self.code);
        write_u32(&mut bytes, 8, self.x as u32);
        write_u32(&mut bytes, 12, self.y as u32);
        write_u32(&mut bytes, 16, self.z as u32);
        write_u32(&mut bytes, 20, self.flags);
        bytes
    }
}

static KEYBOARD: Mutex<Keyboard> = Mutex::new(Keyboard::new());

pub fn drain() {
    KEYBOARD.lock().drain();
    crate::arch::mouse::drain();
}

pub fn poll() -> Option<InputPacket> {
    if let Some(key) = KEYBOARD.lock().poll_key() {
        return Some(InputPacket {
            kind: INPUT_KEY,
            code: key_code(key),
            flags: 1,
            ..InputPacket::default()
        });
    }

    crate::arch::mouse::poll_event().map(|event| InputPacket {
        kind: INPUT_MOUSE,
        x: event.dx as i32,
        y: event.dy as i32,
        z: event.wheel as i32,
        flags: event.buttons as u32,
        ..InputPacket::default()
    })
}

fn key_code(key: Key) -> u32 {
    match key {
        Key::Char(byte) => byte as u32,
        Key::Tab => KEY_TAB,
        Key::Enter => KEY_ENTER,
        Key::Escape => KEY_ESCAPE,
        Key::Backspace => KEY_BACKSPACE,
        Key::F12 => KEY_F12,
        Key::ArrowUp => KEY_ARROW_UP,
        Key::ArrowDown => KEY_ARROW_DOWN,
        Key::ArrowLeft => KEY_ARROW_LEFT,
        Key::ArrowRight => KEY_ARROW_RIGHT,
    }
}

fn write_u32(bytes: &mut [u8; PACKET_BYTES], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
