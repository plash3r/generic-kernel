use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use x86_64::instructions::port::Port;

pub const HZ: u64 = 100;
const PIT_INPUT_HZ: u64 = 1_193_182;

static TICKS: AtomicU64 = AtomicU64::new(0);
static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init() {
    let divisor = (PIT_INPUT_HZ / HZ).clamp(1, u16::MAX as u64) as u16;
    // SAFETY: 0x43/0x40 are the standard 8254 PIT command/channel-0 ports.
    unsafe {
        Port::<u8>::new(0x43).write(0x36);
        let mut channel0 = Port::<u8>::new(0x40);
        channel0.write(divisor as u8);
        channel0.write((divisor >> 8) as u8);
    }
    INITIALIZED.store(true, Ordering::SeqCst);
}

pub fn interrupt() {
    let tick = TICKS.fetch_add(1, Ordering::Relaxed).saturating_add(1);
    crate::task::on_timer_tick(tick);
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

pub fn millis() -> u64 {
    ticks().saturating_mul(1000) / HZ
}

pub fn initialized() -> bool {
    INITIALIZED.load(Ordering::SeqCst)
}

pub fn wait_ticks(count: u64) {
    let target = ticks().saturating_add(count);
    while ticks() < target {
        x86_64::instructions::hlt();
    }
}
