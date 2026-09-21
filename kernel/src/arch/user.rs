use core::{
    arch::global_asm,
    sync::atomic::{AtomicU64, Ordering},
};
use x86_64::VirtAddr;

pub const SYS_EXIT: u64 = 0;
pub const SYS_TICKS: u64 = 1;
pub const SYS_WRITE: u64 = 2;
pub const SYS_DISPLAY_INFO: u64 = 3;
pub const SYS_DISPLAY_PRESENT: u64 = 4;
pub const SYS_INPUT_POLL: u64 = 5;

const EXIT_SENTINEL: u64 = u64::MAX;
pub const ENOSYS: u64 = u64::MAX - 1;
const EBADF: u64 = u64::MAX - 2;
const EFAULT: u64 = u64::MAX - 3;
const E2BIG: u64 = u64::MAX - 4;
const EINVAL: u64 = u64::MAX - 5;
const ENODEV: u64 = u64::MAX - 6;
const MAX_WRITE_BYTES: usize = 4096;
const DISPLAY_INFO_BYTES: usize = 32;

static LAST_CPL: AtomicU64 = AtomicU64::new(0);
static LAST_EXIT: AtomicU64 = AtomicU64::new(0);

#[no_mangle]
static mut GENERIC_USER_KERNEL_RSP: u64 = 0;
#[no_mangle]
static mut GENERIC_USER_EXIT_CODE: u64 = 0;

global_asm!(
    r#"
    .text
    .global generic_enter_user
    .type generic_enter_user,@function
generic_enter_user:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15

    mov [r8], rsp

    push rcx
    push rsi
    pushfq
    pop rax
    or rax, 0x200
    push rax
    push rdx
    push rdi
    iretq
    .size generic_enter_user, .-generic_enter_user

    .global generic_int80_entry
    .type generic_int80_entry,@function
generic_int80_entry:
    push rax
    push rbx
    push rcx
    push rdx
    push rbp
    push rdi
    push rsi
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    mov rdi, [rsp + 112]
    mov rsi, [rsp + 72]
    mov rdx, [rsp + 64]
    mov rcx, [rsp + 88]
    mov r8, [rsp + 128]
    call generic_syscall_dispatch

    cmp rax, -1
    je .Lgeneric_user_exit

    mov [rsp + 112], rax

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rsi
    pop rdi
    pop rbp
    pop rdx
    pop rcx
    pop rbx
    pop rax
    iretq

.Lgeneric_user_exit:
    mov rsp, [rip + GENERIC_USER_KERNEL_RSP]
    mov rax, [rip + GENERIC_USER_EXIT_CODE]
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret
    .size generic_int80_entry, .-generic_int80_entry
"#
);

unsafe extern "C" {
    fn generic_enter_user(
        user_rip: u64,
        user_rsp: u64,
        user_cs: u64,
        user_ss: u64,
        kernel_rsp_slot: *mut u64,
    ) -> u64;
    fn generic_int80_entry();
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Diagnostics {
    pub last_cpl: u8,
    pub last_exit: u64,
}

pub fn syscall_entry_address() -> VirtAddr {
    VirtAddr::new(generic_int80_entry as usize as u64)
}

pub fn diagnostics() -> Diagnostics {
    Diagnostics {
        last_cpl: LAST_CPL.load(Ordering::SeqCst) as u8,
        last_exit: LAST_EXIT.load(Ordering::SeqCst),
    }
}

pub fn enter(user_rip: u64, user_rsp: u64) -> u64 {
    LAST_CPL.store(0, Ordering::SeqCst);
    let (user_cs, user_ss) = crate::arch::interrupts::user_selectors();
    let kernel_rsp_slot = core::ptr::addr_of_mut!(GENERIC_USER_KERNEL_RSP);

    // SAFETY: the process loader guarantees that RIP and RSP point into
    // USER_ACCESSIBLE mappings. The selectors are DPL3 and TSS.RSP0 is valid.
    let exit = unsafe {
        generic_enter_user(
            user_rip,
            user_rsp,
            user_cs as u64,
            user_ss as u64,
            kernel_rsp_slot,
        )
    };

    assert_eq!(
        LAST_CPL.load(Ordering::SeqCst),
        3,
        "userspace syscall did not originate at CPL3"
    );
    LAST_EXIT.store(exit, Ordering::SeqCst);
    exit
}

#[no_mangle]
extern "C" fn generic_syscall_dispatch(
    number: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    caller_cs: u64,
) -> u64 {
    LAST_CPL.store(caller_cs & 3, Ordering::SeqCst);

    match number {
        SYS_EXIT => {
            // SAFETY: syscall entry is an interrupt gate, so interrupts are
            // masked while this single-CPU bootstrap syscall path updates it.
            unsafe {
                GENERIC_USER_EXIT_CODE = arg0;
            }
            EXIT_SENTINEL
        }
        SYS_TICKS => crate::arch::timer::ticks(),
        SYS_WRITE => {
            let Ok(length) = usize::try_from(arg2) else {
                return E2BIG;
            };
            let data = match crate::mm::copy_from_user(arg1, length, MAX_WRITE_BYTES) {
                Ok(data) => data,
                Err("userspace buffer exceeds syscall limit") => return E2BIG,
                Err(_) => return EFAULT,
            };
            match crate::process::write_current_fd(arg0, &data) {
                Ok(written) => written as u64,
                Err(_) => EBADF,
            }
        }
        SYS_DISPLAY_INFO => {
            let Ok(length) = usize::try_from(arg1) else {
                return EINVAL;
            };
            if length < DISPLAY_INFO_BYTES {
                return EINVAL;
            }
            let Some(info) = crate::arch::display::info() else {
                return ENODEV;
            };
            let Ok(width) = u32::try_from(info.width) else {
                return EINVAL;
            };
            let Ok(height) = u32::try_from(info.height) else {
                return EINVAL;
            };
            let mut bytes = [0u8; DISPLAY_INFO_BYTES];
            write_u32(&mut bytes, 0, crate::arch::display::ABI_VERSION);
            write_u32(&mut bytes, 4, width);
            write_u32(&mut bytes, 8, height);
            write_u32(&mut bytes, 12, width);
            write_u32(&mut bytes, 16, 4);
            write_u32(&mut bytes, 20, crate::arch::display::SOURCE_FORMAT_XRGB8888);
            write_u32(&mut bytes, 24, info.native_pixel_format);
            if crate::mm::copy_to_user(arg0, &bytes, DISPLAY_INFO_BYTES).is_err() {
                return EFAULT;
            }
            0
        }
        SYS_DISPLAY_PRESENT => {
            let Ok(length) = usize::try_from(arg1) else {
                return EINVAL;
            };
            match crate::arch::display::present_xrgb8888(arg0, length) {
                Ok(()) => 0,
                Err("invalid userspace read buffer") => EFAULT,
                Err(_) => EINVAL,
            }
        }
        SYS_INPUT_POLL => {
            let Ok(length) = usize::try_from(arg1) else {
                return EINVAL;
            };
            if length < crate::arch::input::PACKET_BYTES {
                return EINVAL;
            }
            let Some(packet) = crate::arch::input::poll() else {
                return 0;
            };
            let bytes = packet.encode();
            if crate::mm::copy_to_user(arg0, &bytes, crate::arch::input::PACKET_BYTES).is_err() {
                return EFAULT;
            }
            1
        }
        _ => ENOSYS,
    }
}

fn write_u32(bytes: &mut [u8; DISPLAY_INFO_BYTES], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
