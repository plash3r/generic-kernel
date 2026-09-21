use alloc::{boxed::Box, vec, vec::Vec};
use core::{
    arch::global_asm,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use spin::Mutex;

const STACK_BYTES: usize = 32 * 1024;
const STACK_WORDS: usize = STACK_BYTES / core::mem::size_of::<u128>();
const QUANTUM_TICKS: u64 = 5;

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static RESCHEDULE_REQUESTED: AtomicBool = AtomicBool::new(false);
static HEARTBEAT: AtomicU64 = AtomicU64::new(0);

global_asm!(
    r#"
    .text
    .global generic_context_switch
    .type generic_context_switch,@function
generic_context_switch:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15

    mov [rdi], rsp
    mov rsp, rsi

    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret
    .size generic_context_switch, .-generic_context_switch
"#
);

unsafe extern "C" {
    fn generic_context_switch(old_rsp: *mut u64, new_rsp: u64);
}

pub type TaskId = u64;
pub type TaskEntry = fn();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Running,
    Ready,
    Sleeping,
    Exited,
}

#[derive(Clone, Copy, Debug)]
enum InternalState {
    Running,
    Ready,
    Sleeping(u64),
    Exited,
}

struct Task {
    id: TaskId,
    name: &'static str,
    state: InternalState,
    rsp: u64,
    _stack: Option<Box<[u128]>>,
    entry: Option<TaskEntry>,
}

pub struct TaskInfo {
    pub id: TaskId,
    pub name: &'static str,
    pub state: TaskState,
    pub wake_tick: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SchedulerStats {
    pub initialized: bool,
    pub total: usize,
    pub running: usize,
    pub ready: usize,
    pub sleeping: usize,
    pub exited: usize,
    pub context_switches: u64,
    pub heartbeat: u64,
}

struct Scheduler {
    tasks: Vec<Task>,
    current: usize,
    next_id: TaskId,
    context_switches: u64,
}

impl Scheduler {
    const fn new() -> Self {
        Self {
            tasks: Vec::new(),
            current: 0,
            next_id: 1,
            context_switches: 0,
        }
    }

    fn wake_sleepers(&mut self, now: u64) {
        for task in &mut self.tasks {
            if let InternalState::Sleeping(until) = task.state {
                if now >= until {
                    task.state = InternalState::Ready;
                }
            }
        }
    }

    fn next_ready(&self) -> Option<usize> {
        let len = self.tasks.len();
        if len <= 1 {
            return None;
        }
        for offset in 1..len {
            let index = (self.current + offset) % len;
            if matches!(self.tasks[index].state, InternalState::Ready) {
                return Some(index);
            }
        }
        None
    }
}

static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

struct Switch {
    old_rsp: *mut u64,
    new_rsp: u64,
}

pub fn init() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler = SCHEDULER.lock();
        assert!(scheduler.tasks.is_empty(), "scheduler initialized twice");
        scheduler.tasks.push(Task {
            id: 0,
            name: "boot",
            state: InternalState::Running,
            rsp: 0,
            _stack: None,
            entry: None,
        });
        scheduler.current = 0;
        scheduler.next_id = 1;
        scheduler.context_switches = 0;
        INITIALIZED.store(true, Ordering::SeqCst);
    });

    spawn("kworker/0", housekeeping_task).expect("failed to spawn kernel worker");
    crate::log!(
        "[ok] scheduler cooperative round-robin, {} KiB kernel stacks, {}-tick quantum\n",
        STACK_BYTES / 1024,
        QUANTUM_TICKS
    );
}

pub fn spawn(name: &'static str, entry: TaskEntry) -> Option<TaskId> {
    if !INITIALIZED.load(Ordering::SeqCst) {
        return None;
    }

    let mut stack = vec![0u128; STACK_WORDS].into_boxed_slice();
    let rsp = prepare_stack(&mut stack);

    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut scheduler = SCHEDULER.lock();
        let id = scheduler.next_id;
        scheduler.next_id = scheduler.next_id.checked_add(1)?;
        scheduler.tasks.push(Task {
            id,
            name,
            state: InternalState::Ready,
            rsp,
            _stack: Some(stack),
            entry: Some(entry),
        });
        Some(id)
    })
}

pub fn on_timer_tick(tick: u64) {
    if INITIALIZED.load(Ordering::Relaxed) && tick % QUANTUM_TICKS == 0 {
        RESCHEDULE_REQUESTED.store(true, Ordering::Release);
    }
}

pub fn checkpoint() {
    if RESCHEDULE_REQUESTED.swap(false, Ordering::AcqRel) {
        yield_now();
    }
}

pub fn yield_now() {
    if !INITIALIZED.load(Ordering::SeqCst) {
        return;
    }

    let interrupts_enabled = x86_64::instructions::interrupts::are_enabled();
    x86_64::instructions::interrupts::disable();

    let switch = {
        let mut scheduler = SCHEDULER.lock();
        scheduler.wake_sleepers(crate::arch::timer::ticks());

        let current = scheduler.current;
        if matches!(scheduler.tasks[current].state, InternalState::Running) {
            scheduler.tasks[current].state = InternalState::Ready;
        }

        let Some(next) = scheduler.next_ready() else {
            scheduler.tasks[current].state = InternalState::Running;
            drop(scheduler);
            if interrupts_enabled {
                x86_64::instructions::interrupts::enable();
            }
            return;
        };

        scheduler.tasks[next].state = InternalState::Running;
        scheduler.current = next;
        scheduler.context_switches = scheduler.context_switches.saturating_add(1);

        let new_rsp = scheduler.tasks[next].rsp;
        let old_rsp = &mut scheduler.tasks[current].rsp as *mut u64;
        Switch { old_rsp, new_rsp }
    };

    if interrupts_enabled {
        x86_64::instructions::interrupts::enable();
    }

    // SAFETY: both contexts are scheduler-owned kernel stacks. The old stack
    // remains allocated for the lifetime of its task, and the new stack was
    // initialized with the exact register layout expected by the assembly shim.
    unsafe {
        generic_context_switch(switch.old_rsp, switch.new_rsp);
    }
}

pub fn sleep_ticks(ticks: u64) {
    if ticks == 0 {
        yield_now();
        return;
    }
    if !INITIALIZED.load(Ordering::SeqCst) {
        crate::arch::timer::wait_ticks(ticks);
        return;
    }

    let wake = crate::arch::timer::ticks().saturating_add(ticks);
    let interrupts_enabled = x86_64::instructions::interrupts::are_enabled();
    x86_64::instructions::interrupts::disable();

    let switch = {
        let mut scheduler = SCHEDULER.lock();
        scheduler.wake_sleepers(crate::arch::timer::ticks());

        let current = scheduler.current;
        scheduler.tasks[current].state = InternalState::Sleeping(wake);

        let Some(next) = scheduler.next_ready() else {
            scheduler.tasks[current].state = InternalState::Running;
            drop(scheduler);
            if interrupts_enabled {
                x86_64::instructions::interrupts::enable();
            }
            while crate::arch::timer::ticks() < wake {
                x86_64::instructions::hlt();
            }
            return;
        };

        scheduler.tasks[next].state = InternalState::Running;
        scheduler.current = next;
        scheduler.context_switches = scheduler.context_switches.saturating_add(1);

        let new_rsp = scheduler.tasks[next].rsp;
        let old_rsp = &mut scheduler.tasks[current].rsp as *mut u64;
        Switch { old_rsp, new_rsp }
    };

    if interrupts_enabled {
        x86_64::instructions::interrupts::enable();
    }

    // SAFETY: see yield_now; this task's saved context remains valid while it sleeps.
    unsafe {
        generic_context_switch(switch.old_rsp, switch.new_rsp);
    }
}

pub fn stats() -> SchedulerStats {
    if !INITIALIZED.load(Ordering::SeqCst) {
        return SchedulerStats::default();
    }

    x86_64::instructions::interrupts::without_interrupts(|| {
        let scheduler = SCHEDULER.lock();
        let mut stats = SchedulerStats {
            initialized: true,
            total: scheduler.tasks.len(),
            context_switches: scheduler.context_switches,
            heartbeat: HEARTBEAT.load(Ordering::Relaxed),
            ..SchedulerStats::default()
        };

        for task in &scheduler.tasks {
            match task.state {
                InternalState::Running => stats.running += 1,
                InternalState::Ready => stats.ready += 1,
                InternalState::Sleeping(_) => stats.sleeping += 1,
                InternalState::Exited => stats.exited += 1,
            }
        }
        stats
    })
}

pub fn tasks() -> Vec<TaskInfo> {
    if !INITIALIZED.load(Ordering::SeqCst) {
        return Vec::new();
    }

    x86_64::instructions::interrupts::without_interrupts(|| {
        let scheduler = SCHEDULER.lock();
        scheduler
            .tasks
            .iter()
            .map(|task| {
                let (state, wake_tick) = match task.state {
                    InternalState::Running => (TaskState::Running, None),
                    InternalState::Ready => (TaskState::Ready, None),
                    InternalState::Sleeping(tick) => (TaskState::Sleeping, Some(tick)),
                    InternalState::Exited => (TaskState::Exited, None),
                };
                TaskInfo {
                    id: task.id,
                    name: task.name,
                    state,
                    wake_tick,
                }
            })
            .collect()
    })
}

fn prepare_stack(stack: &mut [u128]) -> u64 {
    let end = stack.as_mut_ptr() as u64 + (stack.len() * core::mem::size_of::<u128>()) as u64;
    debug_assert_eq!(end & 0xf, 0);

    // The context switch restores six callee-saved registers then RETs. Keep
    // the synthetic post-RET stack at 8 mod 16, matching SysV function entry.
    let return_slot = end - 16;
    let initial_rsp = end - 64;

    // SAFETY: the 32 KiB allocation is 16-byte aligned and these slots are
    // within its final 64 bytes. The stack stays owned by the Task.
    unsafe {
        core::ptr::write(return_slot as *mut u64, task_trampoline as usize as u64);
    }

    initial_rsp
}

extern "C" fn task_trampoline() -> ! {
    let entry = x86_64::instructions::interrupts::without_interrupts(|| {
        let scheduler = SCHEDULER.lock();
        scheduler.tasks[scheduler.current]
            .entry
            .expect("kernel task missing entry point")
    });

    entry();
    exit_current()
}

fn exit_current() -> ! {
    let interrupts_enabled = x86_64::instructions::interrupts::are_enabled();
    x86_64::instructions::interrupts::disable();

    let switch = {
        let mut scheduler = SCHEDULER.lock();
        scheduler.wake_sleepers(crate::arch::timer::ticks());

        let current = scheduler.current;
        scheduler.tasks[current].state = InternalState::Exited;

        let next = scheduler
            .next_ready()
            .expect("kernel task exited with no runnable successor");
        scheduler.tasks[next].state = InternalState::Running;
        scheduler.current = next;
        scheduler.context_switches = scheduler.context_switches.saturating_add(1);

        let new_rsp = scheduler.tasks[next].rsp;
        let old_rsp = &mut scheduler.tasks[current].rsp as *mut u64;
        Switch { old_rsp, new_rsp }
    };

    if interrupts_enabled {
        x86_64::instructions::interrupts::enable();
    }

    // SAFETY: the exited context is retained but never scheduled again. The
    // successor is a live scheduler-owned kernel stack.
    unsafe {
        generic_context_switch(switch.old_rsp, switch.new_rsp);
    }

    panic!("exited kernel task resumed")
}

fn housekeeping_task() {
    loop {
        HEARTBEAT.fetch_add(1, Ordering::Relaxed);
        sleep_ticks(crate::arch::timer::HZ);
    }
}

#[cfg(feature = "smoke")]
static SMOKE_A: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "smoke")]
static SMOKE_B: AtomicU64 = AtomicU64::new(0);

#[cfg(feature = "smoke")]
fn smoke_a() {
    for _ in 0..4 {
        SMOKE_A.fetch_add(1, Ordering::Relaxed);
        yield_now();
    }
}

#[cfg(feature = "smoke")]
fn smoke_b() {
    for _ in 0..6 {
        SMOKE_B.fetch_add(1, Ordering::Relaxed);
        yield_now();
    }
}

#[cfg(feature = "smoke")]
pub fn smoke_test() {
    SMOKE_A.store(0, Ordering::Relaxed);
    SMOKE_B.store(0, Ordering::Relaxed);
    spawn("smoke/a", smoke_a).expect("scheduler smoke task A");
    spawn("smoke/b", smoke_b).expect("scheduler smoke task B");

    for _ in 0..64 {
        if SMOKE_A.load(Ordering::Relaxed) == 4 && SMOKE_B.load(Ordering::Relaxed) == 6 {
            break;
        }
        yield_now();
    }

    assert_eq!(SMOKE_A.load(Ordering::Relaxed), 4);
    assert_eq!(SMOKE_B.load(Ordering::Relaxed), 6);
    let stats = stats();
    assert!(stats.context_switches >= 10);
    crate::log!(
        "[ok] scheduler context switch: {} switches, {} task(s)\n",
        stats.context_switches,
        stats.total
    );
}
