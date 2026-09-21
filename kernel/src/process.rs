use alloc::{string::String, vec, vec::Vec};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use kernel_core::{elf::ElfImage, PAGE_SIZE};
use spin::Mutex;

const USER_STACK_BASE: u64 = 0x0000_0000_0080_0000;
const USER_LIMIT: u64 = 1u64 << 39;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Exited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FdKind {
    Stdin,
    Stdout,
    Stderr,
}

#[derive(Clone, Debug)]
struct Process {
    pid: u64,
    name: String,
    state: ProcessState,
    entry: u64,
    cr3: u64,
    fds: [Option<FdKind>; 3],
    exit_code: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct ProcessInfo {
    pub pid: u64,
    pub name: String,
    pub state: ProcessState,
    pub entry: u64,
    pub cr3: u64,
    pub fd_count: usize,
    pub exit_code: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub probe_ok: bool,
    pub process_count: usize,
    pub last_exit: u64,
    pub kernel_cr3: u64,
    pub process_cr3: u64,
    pub isolated_address_space: bool,
}

static PROCESSES: Mutex<Vec<Process>> = Mutex::new(Vec::new());
static NEXT_PID: AtomicU64 = AtomicU64::new(1);
static PROBE_OK: AtomicBool = AtomicBool::new(false);
static LAST_EXIT: AtomicU64 = AtomicU64::new(0);
static CURRENT_PID: AtomicU64 = AtomicU64::new(0);

pub fn init_probe() {
    if PROBE_OK.load(Ordering::SeqCst) {
        return;
    }

    let bytes = crate::vfs::read_file("/bin/init").expect("/bin/init missing from initramfs");
    let image = ElfImage::parse(&bytes).expect("invalid /bin/init ELF");

    let kernel_space = crate::mm::active_address_space();
    let process_space =
        crate::mm::create_user_address_space().expect("failed to create /bin/init address space");
    assert_ne!(
        kernel_space.root, process_space.root,
        "userspace process reused the kernel CR3"
    );

    load_image(&image, process_space).expect("failed to map /bin/init");
    crate::mm::map_user_page(process_space, USER_STACK_BASE, true, false, &[])
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
            cr3: process_space.root,
            fds: [
                Some(FdKind::Stdin),
                Some(FdKind::Stdout),
                Some(FdKind::Stderr),
            ],
            exit_code: None,
        });
    }

    crate::mm::activate_address_space(process_space).expect("failed to activate /bin/init CR3");
    assert_eq!(crate::mm::active_address_space(), process_space);
    CURRENT_PID.store(pid, Ordering::SeqCst);

    let exit = crate::arch::user::enter(image.entry(), stack_top);

    CURRENT_PID.store(0, Ordering::SeqCst);
    crate::mm::activate_address_space(kernel_space).expect("failed to restore kernel CR3");
    assert_eq!(crate::mm::active_address_space(), kernel_space);

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
        "[ok] process address space: kernel CR3={:#x}, pid1 CR3={:#x}, private page-table tree\n",
        kernel_space.root,
        process_space.root
    );
    crate::log!(
        "[ok] userspace ELF /bin/init: entry={:#x}, CPL{}, exit={}\n",
        image.entry(),
        user.last_cpl,
        exit
    );
}

pub fn diagnostics() -> Diagnostics {
    let kernel_cr3 = crate::mm::active_address_space().root;
    let processes = PROCESSES.lock();
    let process_cr3 = processes.first().map(|process| process.cr3).unwrap_or(0);
    Diagnostics {
        probe_ok: PROBE_OK.load(Ordering::SeqCst),
        process_count: processes.len(),
        last_exit: LAST_EXIT.load(Ordering::SeqCst),
        kernel_cr3,
        process_cr3,
        isolated_address_space: process_cr3 != 0 && process_cr3 != kernel_cr3,
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
            cr3: process.cr3,
            fd_count: process.fds.iter().flatten().count(),
            exit_code: process.exit_code,
        })
        .collect()
}

pub fn write_current_fd(fd: u64, data: &[u8]) -> Result<usize, &'static str> {
    let pid = CURRENT_PID.load(Ordering::SeqCst);
    if pid == 0 {
        return Err("no current userspace process");
    }

    let kind = {
        let processes = PROCESSES.lock();
        let process = processes
            .iter()
            .find(|process| process.pid == pid && process.state == ProcessState::Running)
            .ok_or("current process is not runnable")?;
        let index = usize::try_from(fd).map_err(|_| "file descriptor out of range")?;
        process
            .fds
            .get(index)
            .copied()
            .flatten()
            .ok_or("bad file descriptor")?
    };

    match kind {
        FdKind::Stdout | FdKind::Stderr => {
            crate::arch::serial::write_bytes(data);
            Ok(data.len())
        }
        FdKind::Stdin => Err("file descriptor is not writable"),
    }
}

fn load_image(
    image: &ElfImage<'_>,
    address_space: crate::mm::AddressSpace,
) -> Result<(), &'static str> {
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

            crate::mm::map_user_page(
                address_space,
                page_address,
                segment.writable,
                segment.executable,
                &initial,
            )?;
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
