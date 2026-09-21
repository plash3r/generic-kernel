use alloc::{string::String, vec, vec::Vec};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use kernel_core::{elf::ElfImage, PAGE_SIZE};
use spin::Mutex;

const USER_STACK_BASE: u64 = 0x0000_0000_0080_0000;
const USER_LIMIT: u64 = 0x0000_8000_0000_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Exited,
}

#[derive(Clone, Debug)]
struct Process {
    pid: u64,
    name: String,
    state: ProcessState,
    entry: u64,
    exit_code: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ProcessInfo {
    pub pid: u64,
    pub name: String,
    pub state: ProcessState,
    pub entry: u64,
    pub exit_code: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub probe_ok: bool,
    pub process_count: usize,
    pub last_exit: u64,
}

static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
static NEXT_PID: AtomicU64 = AtomicU64::new(1);
static PROBE_OK: AtomicBool = AtomicBool::new(false);
static LAST_EXIT: AtomicU64 = AtomicU64::new(0);

pub fn init_probe() {
    if PROBE_OK.load(Ordering::SeqCst) {
        return;
    }

    let bytes = crate::vfs::read_file("/bin/init").expect("/bin/init missing from initramfs");
    let image = ElfImage::parse(&bytes).expect("invalid /bin/init ELF");
    load_image(&image).expect("failed to map /bin/init");

    crate::mm::map_user_page(USER_STACK_BASE, true, false, &[])
        .expect("failed to map userspace stack");
    let stack_top = USER_STACK_BASE + PAGE_SIZE;

    let pid = NEXT_PID.fetch_add(1, Ordering::SeqCst);
    {
        let mut processes = PROCESSES.lock();
        processes.push(Process {
            pid,
            name: String::from("/bin/init"),
            state: ProcessState::Running,
            entry: image.entry(),
            exit_code: None,
        });
    }

    let exit = crate::arch::user::enter(image.entry(), stack_top);
    assert!(
        exit > 0 && exit != crate::arch::user::ENOSYS,
        "userspace init syscall path failed"
    );

    {
        let mut processes = PROCESSES.lock();
        let process = processes
            .iter_mut()
            .find(|process| process.pid == pid)
            .expect("userspace process disappeared");
        process.state = ProcessState::Exited;
        process.exit_code = Some(exit);
    }

    LAST_EXIT.store(exit, Ordering::SeqCst);
    PROBE_OK.store(true, Ordering::SeqCst);

    let user = crate::arch::user::diagnostics();
    crate::log!(
        "[ok] userspace ELF /bin/init: entry={:#x}, CPL{}, exit={}\n",
        image.entry(),
        user.last_cpl,
        exit
    );
}

pub fn diagnostics() -> Diagnostics {
    Diagnostics {
        probe_ok: PROBE_OK.load(Ordering::SeqCst),
        process_count: PROCESSES.lock().len(),
        last_exit: LAST_EXIT.load(Ordering::SeqCst),
    }
}

pub fn processes() -> Vec<ProcessInfo> {
    PROCESSES
        .lock()
        .iter()
        .map(|process| ProcessInfo {
            pid: process.pid,
            name: process.name.clone(),
            state: process.state,
            entry: process.entry,
            exit_code: process.exit_code,
        })
        .collect()
}

fn load_image(image: &ElfImage<'_>) -> Result<(), &'static str> {
    if image.entry() >= USER_LIMIT {
        return Err("ELF entry is outside userspace");
    }

    for segment in image.segments() {
        if segment.memory_size == 0 {
            continue;
        }
        if segment.virtual_address >= USER_LIMIT {
            return Err("ELF segment starts outside userspace");
        }
        let segment_end = segment
            .virtual_address
            .checked_add(segment.memory_size)
            .ok_or("ELF segment address overflow")?;
        if segment_end > USER_LIMIT {
            return Err("ELF segment ends outside userspace");
        }

        let page_start = segment.virtual_address & !(PAGE_SIZE - 1);
        let page_end = align_up(segment_end, PAGE_SIZE).ok_or("ELF page range overflow")?;
        let file_end = segment
            .virtual_address
            .checked_add(segment.file_data.len() as u64)
            .ok_or("ELF file segment overflow")?;

        let mut page_address = page_start;
        while page_address < page_end {
            let mut initial = vec![0u8; PAGE_SIZE as usize];
            let copy_start = page_address.max(segment.virtual_address);
            let copy_end = (page_address + PAGE_SIZE).min(file_end);

            if copy_start < copy_end {
                let source_start = (copy_start - segment.virtual_address) as usize;
                let source_end = (copy_end - segment.virtual_address) as usize;
                let destination = (copy_start - page_address) as usize;
                initial[destination..destination + (source_end - source_start)]
                    .copy_from_slice(&segment.file_data[source_start..source_end]);
            }

            crate::mm::map_user_page(page_address, segment.writable, segment.executable, &initial)?;
            page_address += PAGE_SIZE;
        }
    }

    Ok(())
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    value
        .checked_add(alignment - 1)
        .map(|rounded| rounded & !(alignment - 1))
}
