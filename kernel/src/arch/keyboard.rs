use x86_64::instructions::port::Port;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(u8),
    Enter,
    Backspace,
}

pub struct Keyboard {
    shift: bool,
    caps_lock: bool,
    extended: bool,
}

impl Keyboard {
    pub const fn new() -> Self {
        Self {
            shift: false,
            caps_lock: false,
            extended: false,
        }
    }

    pub fn drain(&mut self) {
        for _ in 0..64 {
            if !self.data_ready() {
                break;
            }
            let _ = self.read_scancode();
        }
    }

    pub fn read_key_blocking(&mut self) -> Key {
        loop {
            if !self.data_ready() {
                core::hint::spin_loop();
                continue;
            }
            if let Some(key) = self.read_scancode() {
                return key;
            }
        }
    }

    fn data_ready(&self) -> bool {
        // SAFETY: port 0x64 is the standard i8042 status register.
        unsafe { Port::<u8>::new(0x64).read() & 0x01 != 0 }
    }

    fn read_scancode(&mut self) -> Option<Key> {
        // SAFETY: status bit 0 was checked before reading the i8042 data port.
        let code = unsafe { Port::<u8>::new(0x60).read() };

        if code == 0xe0 {
            self.extended = true;
            return None;
        }
        if self.extended {
            self.extended = false;
            return None;
        }

        match code {
            0x2a | 0x36 => {
                self.shift = true;
                return None;
            }
            0xaa | 0xb6 => {
                self.shift = false;
                return None;
            }
            0x3a => {
                self.caps_lock = !self.caps_lock;
                return None;
            }
            0x1c => return Some(Key::Enter),
            0x0e => return Some(Key::Backspace),
            _ if code & 0x80 != 0 => return None,
            _ => {}
        }

        self.scancode_to_ascii(code).map(Key::Char)
    }

    fn scancode_to_ascii(&self, code: u8) -> Option<u8> {
        let letter = match code {
            0x10 => Some(b'q'),
            0x11 => Some(b'w'),
            0x12 => Some(b'e'),
            0x13 => Some(b'r'),
            0x14 => Some(b't'),
            0x15 => Some(b'y'),
            0x16 => Some(b'u'),
            0x17 => Some(b'i'),
            0x18 => Some(b'o'),
            0x19 => Some(b'p'),
            0x1e => Some(b'a'),
            0x1f => Some(b's'),
            0x20 => Some(b'd'),
            0x21 => Some(b'f'),
            0x22 => Some(b'g'),
            0x23 => Some(b'h'),
            0x24 => Some(b'j'),
            0x25 => Some(b'k'),
            0x26 => Some(b'l'),
            0x2c => Some(b'z'),
            0x2d => Some(b'x'),
            0x2e => Some(b'c'),
            0x2f => Some(b'v'),
            0x30 => Some(b'b'),
            0x31 => Some(b'n'),
            0x32 => Some(b'm'),
            _ => None,
        };

        if let Some(mut byte) = letter {
            if self.shift ^ self.caps_lock {
                byte = byte.to_ascii_uppercase();
            }
            return Some(byte);
        }

        let pair = match code {
            0x02 => (b'1', b'!'),
            0x03 => (b'2', b'@'),
            0x04 => (b'3', b'#'),
            0x05 => (b'4', b'$'),
            0x06 => (b'5', b'%'),
            0x07 => (b'6', b'^'),
            0x08 => (b'7', b'&'),
            0x09 => (b'8', b'*'),
            0x0a => (b'9', b'('),
            0x0b => (b'0', b')'),
            0x0c => (b'-', b'_'),
            0x0d => (b'=', b'+'),
            0x1a => (b'[', b'{'),
            0x1b => (b']', b'}'),
            0x27 => (b';', b':'),
            0x28 => (b'\'', b'"'),
            0x29 => (0x60, b'~'),
            0x2b => (b'\\', b'|'),
            0x33 => (b',', b'<'),
            0x34 => (b'.', b'>'),
            0x35 => (b'/', b'?'),
            0x39 => (b' ', b' '),
            0x0f => (b' ', b' '),
            _ => return None,
        };

        Some(if self.shift { pair.1 } else { pair.0 })
    }
}
