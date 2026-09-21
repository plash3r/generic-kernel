use spin::Mutex;

const QUEUE_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(u8),
    Tab,
    Enter,
    Backspace,
    Escape,
    F12,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

struct ScancodeQueue {
    bytes: [u8; QUEUE_CAPACITY],
    read: usize,
    write: usize,
    len: usize,
}

impl ScancodeQueue {
    const fn new() -> Self {
        Self {
            bytes: [0; QUEUE_CAPACITY],
            read: 0,
            write: 0,
            len: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.len == QUEUE_CAPACITY {
            self.read = (self.read + 1) % QUEUE_CAPACITY;
            self.len -= 1;
        }
        self.bytes[self.write] = byte;
        self.write = (self.write + 1) % QUEUE_CAPACITY;
        self.len += 1;
    }

    fn pop(&mut self) -> Option<u8> {
        if self.len == 0 {
            return None;
        }
        let byte = self.bytes[self.read];
        self.read = (self.read + 1) % QUEUE_CAPACITY;
        self.len -= 1;
        Some(byte)
    }

    fn clear(&mut self) {
        self.read = 0;
        self.write = 0;
        self.len = 0;
    }
}

static SCANCODES: Mutex<ScancodeQueue> = Mutex::new(ScancodeQueue::new());

pub fn interrupt() {
    if let Some(code) = crate::arch::ps2::read_interrupt_data(false) {
        SCANCODES.lock().push(code);
    }
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
        x86_64::instructions::interrupts::without_interrupts(|| SCANCODES.lock().clear());
    }

    pub fn poll_key(&mut self) -> Option<Key> {
        loop {
            let code =
                x86_64::instructions::interrupts::without_interrupts(|| SCANCODES.lock().pop())?;
            if let Some(key) = self.decode_scancode(code) {
                return Some(key);
            }
        }
    }

    pub fn read_key_blocking(&mut self) -> Key {
        loop {
            if let Some(key) = self.poll_key() {
                return key;
            }
            crate::task::checkpoint();
            x86_64::instructions::hlt();
        }
    }

    fn decode_scancode(&mut self, code: u8) -> Option<Key> {
        if code == 0xe0 {
            self.extended = true;
            return None;
        }

        if self.extended {
            self.extended = false;
            if code & 0x80 != 0 {
                return None;
            }
            return match code {
                0x48 => Some(Key::ArrowUp),
                0x50 => Some(Key::ArrowDown),
                0x4b => Some(Key::ArrowLeft),
                0x4d => Some(Key::ArrowRight),
                _ => None,
            };
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
            0x01 => return Some(Key::Escape),
            0x0f => return Some(Key::Tab),
            0x1c => return Some(Key::Enter),
            0x0e => return Some(Key::Backspace),
            0x58 => return Some(Key::F12),
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
            _ => return None,
        };

        Some(if self.shift { pair.1 } else { pair.0 })
    }
}
