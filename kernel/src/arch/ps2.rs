use crate::arch::mouse;
use x86_64::instructions::port::Port;

const DATA: u16 = 0x60;
const STATUS_COMMAND: u16 = 0x64;
const STATUS_OUTPUT_FULL: u8 = 1 << 0;
const STATUS_INPUT_FULL: u8 = 1 << 1;
const STATUS_MOUSE_DATA: u8 = 1 << 5;
const ACK: u8 = 0xfa;
const WAIT_LIMIT: usize = 200_000;

#[derive(Clone, Copy, Debug)]
pub struct Status {
    pub keyboard: bool,
    pub mouse: bool,
    pub wheel: bool,
}

pub fn init() -> Status {
    let _ = command(0xad);
    let _ = command(0xa7);
    flush_output();

    let config = read_config().unwrap_or(0);
    let quiet = (config & !0x03) | 0x40;
    let _ = write_config(quiet);

    let keyboard_enabled = command(0xae);
    let mouse_port_enabled = command(0xa8);

    let active = (quiet | 0x01 | if mouse_port_enabled { 0x02 } else { 0 }) & !0x30;
    let _ = write_config(active);

    let keyboard = if keyboard_enabled {
        device_command(false, 0xf4).unwrap_or(false)
    } else {
        false
    };

    let mut mouse_present = false;
    let mut wheel = false;
    if mouse_port_enabled && device_command(true, 0xf6).unwrap_or(false) {
        mouse_present = true;

        let _ = mouse_set_sample_rate(200);
        let _ = mouse_set_sample_rate(100);
        let _ = mouse_set_sample_rate(80);
        if let Some(id) = mouse_get_id() {
            wheel = id == 3 || id == 4;
        }
        mouse::set_wheel_mode(wheel);
        if !device_command(true, 0xf4).unwrap_or(false) {
            mouse_present = false;
        }
    }

    crate::log!(
        "[ok] PS/2 input keyboard={} mouse={} wheel={}\n",
        keyboard,
        mouse_present,
        wheel
    );
    Status {
        keyboard,
        mouse: mouse_present,
        wheel,
    }
}

pub fn read_interrupt_data(expect_mouse: bool) -> Option<u8> {
    // SAFETY: status/data are the architectural i8042 ports.
    unsafe {
        let status = Port::<u8>::new(STATUS_COMMAND).read();
        if status & STATUS_OUTPUT_FULL == 0 {
            return None;
        }
        if ((status & STATUS_MOUSE_DATA) != 0) != expect_mouse {
            return None;
        }
        Some(Port::<u8>::new(DATA).read())
    }
}

fn mouse_set_sample_rate(rate: u8) -> bool {
    device_command(true, 0xf3).unwrap_or(false) && device_command(true, rate).unwrap_or(false)
}

fn mouse_get_id() -> Option<u8> {
    if !device_command(true, 0xf2).ok()? {
        return None;
    }
    read_data(true)
}

fn device_command(mouse: bool, byte: u8) -> Result<bool, ()> {
    if mouse && !command(0xd4) {
        return Err(());
    }
    if !write_data(byte) {
        return Err(());
    }
    Ok(read_data(mouse) == Some(ACK))
}

fn read_config() -> Option<u8> {
    if !command(0x20) {
        return None;
    }
    read_data(false)
}

fn write_config(value: u8) -> bool {
    command(0x60) && write_data(value)
}

fn command(value: u8) -> bool {
    if !wait_input_clear() {
        return false;
    }
    // SAFETY: port 0x64 is the i8042 command register.
    unsafe { Port::<u8>::new(STATUS_COMMAND).write(value) };
    true
}

fn write_data(value: u8) -> bool {
    if !wait_input_clear() {
        return false;
    }
    // SAFETY: port 0x60 is the i8042 data register.
    unsafe { Port::<u8>::new(DATA).write(value) };
    true
}

fn read_data(expect_mouse: bool) -> Option<u8> {
    for _ in 0..WAIT_LIMIT {
        // SAFETY: status/data are the architectural i8042 ports.
        let status = unsafe { Port::<u8>::new(STATUS_COMMAND).read() };
        if status & STATUS_OUTPUT_FULL != 0 {
            let byte = unsafe { Port::<u8>::new(DATA).read() };
            if ((status & STATUS_MOUSE_DATA) != 0) == expect_mouse {
                return Some(byte);
            }
        }
        core::hint::spin_loop();
    }
    None
}

fn wait_input_clear() -> bool {
    for _ in 0..WAIT_LIMIT {
        // SAFETY: port 0x64 is the i8042 status register.
        if unsafe { Port::<u8>::new(STATUS_COMMAND).read() } & STATUS_INPUT_FULL == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn flush_output() {
    for _ in 0..64 {
        // SAFETY: status/data are the architectural i8042 ports.
        let status = unsafe { Port::<u8>::new(STATUS_COMMAND).read() };
        if status & STATUS_OUTPUT_FULL == 0 {
            break;
        }
        let _ = unsafe { Port::<u8>::new(DATA).read() };
    }
}
