use spin::Mutex;

const QUEUE_CAPACITY: usize = 128;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MouseEvent {
    pub dx: i16,
    pub dy: i16,
    pub wheel: i8,
    pub buttons: u8,
}

struct MouseState {
    packet: [u8; 4],
    packet_index: usize,
    packet_len: usize,
    queue: [MouseEvent; QUEUE_CAPACITY],
    read: usize,
    write: usize,
    len: usize,
}

impl MouseState {
    const fn new() -> Self {
        Self {
            packet: [0; 4],
            packet_index: 0,
            packet_len: 3,
            queue: [MouseEvent {
                dx: 0,
                dy: 0,
                wheel: 0,
                buttons: 0,
            }; QUEUE_CAPACITY],
            read: 0,
            write: 0,
            len: 0,
        }
    }

    fn push_byte(&mut self, byte: u8) {
        if self.packet_index == 0 && byte & 0x08 == 0 {
            return;
        }
        self.packet[self.packet_index] = byte;
        self.packet_index += 1;
        if self.packet_index < self.packet_len {
            return;
        }
        self.packet_index = 0;

        let first = self.packet[0];
        if first & 0xc0 != 0 {
            return;
        }

        let dx = self.packet[1] as i8 as i16;
        let dy = -(self.packet[2] as i8 as i16);
        let wheel = if self.packet_len == 4 {
            let nibble = self.packet[3] & 0x0f;
            if nibble & 0x08 != 0 {
                (nibble | 0xf0) as i8
            } else {
                nibble as i8
            }
        } else {
            0
        };
        let event = MouseEvent {
            dx,
            dy,
            wheel,
            buttons: first & 0x07,
        };

        if self.len == QUEUE_CAPACITY {
            self.read = (self.read + 1) % QUEUE_CAPACITY;
            self.len -= 1;
        }
        self.queue[self.write] = event;
        self.write = (self.write + 1) % QUEUE_CAPACITY;
        self.len += 1;
    }

    fn pop(&mut self) -> Option<MouseEvent> {
        if self.len == 0 {
            return None;
        }
        let event = self.queue[self.read];
        self.read = (self.read + 1) % QUEUE_CAPACITY;
        self.len -= 1;
        Some(event)
    }
}

static MOUSE: Mutex<MouseState> = Mutex::new(MouseState::new());

pub fn set_wheel_mode(enabled: bool) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut mouse = MOUSE.lock();
        mouse.packet_len = if enabled { 4 } else { 3 };
        mouse.packet_index = 0;
    });
}

pub fn interrupt_byte(byte: u8) {
    MOUSE.lock().push_byte(byte);
}

pub fn poll_event() -> Option<MouseEvent> {
    x86_64::instructions::interrupts::without_interrupts(|| MOUSE.lock().pop())
}

pub fn drain() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut mouse = MOUSE.lock();
        mouse.read = 0;
        mouse.write = 0;
        mouse.len = 0;
        mouse.packet_index = 0;
    });
}
