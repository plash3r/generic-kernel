use core::{
    arch::global_asm,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use x86_64::VirtAddr;

pub const SYS_EXIT: u64 = 0;
pub const SYS_TICKS: u64 = 1;

const USER_CODE: u64 = 0x0000_0000_0040_0000;
const USER_STACK: u64 = 0x0000_0000_0080_0000;
const EXIT_SENTINEL: u64 = u64::MAX;
const ENOSYS: u64 = u64::MAX - 1;

static PROBE_OK: AtomicBool = AtomicBool::new(false);
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
    mov rdx, [rsp + 128]
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
    pub probe_ok: bool,
    pub last_cpl: u8,
    pub last_exit: u64,
}

pub fn syscall_entry_address() -> VirtAddr {
    VirtAddr::new(generic_int80_entry as usize as u64)
}

pub fn diagnostics() -> Diagnostics {
    Diagnostics {
        probe_ok: PROBE_OK.load(Ordering::SeqCst),
        last_cpl: LAST_CPL.load(Ordering::SeqCst) as u8,
        last_exit: LAST_EXIT.load(Ordering::SeqCst),
    }
}

pub fn probe() {
    if PROBE_OK.load(Ordering::SeqCst) {
        return;
    }

    // mov rax, SYS_TICKS
    // int 0x80
    // mov rdi, rax
    // mov rax, SYS_EXIT
    // int 0x80
    // ud2
    let code: [u8; 29] = [
        0x48, 0xb8, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xcd, 0x80, 0x48, 0x89,
        0xc7, 0x48, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xcd, 0x80, 0x0f,
        0x0b,
    ];

    crate::mm::map_user_page(USER_CODE, false, true, &code)
        .expect("failed to map ring3 probe code");
    crate::mm::map_user_page(USER_STACK, true, false, &[])
        .expect("failed to map ring3 probe stack");

    let (user_cs, user_ss) = crate::arch::interrupts::user_selectors();
    let kernel_rsp_slot = core::ptr::addr_of_mut!(GENERIC_USER_KERNEL_RSP);

    // SAFETY: code/stack pages are mapped USER_ACCESSIBLE with appropriate
    // permissions; the selectors are DPL3 descriptors and TSS.RSP0 is valid.
    let exit = unsafe {
        generic_enter_user(
            USER_CODE,
            USER_STACK + kernel_core::PAGE_SIZE,
            user_cs as u64,
            user_ss as u64,
            kernel_rsp_slot,
        )
    };

    let cpl = LAST_CPL.load(Ordering::SeqCst);
    assert_eq!(cpl, 3, "ring3 probe syscall did not originate at CPL3");
    assert!(exit > 0 && exit != ENOSYS, "ring3 ticks syscall failed");
    LAST_EXIT.store(exit, Ordering::SeqCst);
    PROBE_OK.store(true, Ordering::SeqCst);

    crate::log!(
        "[ok] ring3 syscall probe: CPL{}, ticks={}, int 0x80 exit\n",
        cpl,
        exit
    );
}

#[no_mangle]
extern "C" fn generic_syscall_dispatch(number: u64, arg0: u64, caller_cs: u64) -> u64 {
    LAST_CPL.store(caller_cs & 3, Ordering::SeqCst);

    match number {
        SYS_EXIT => {
            // SAFETY: syscall entry runs with interrupts masked on the single
            // bootstrap CPU. The assembly exit path immediately consumes it.
            unsafe {
                GENERIC_USER_EXIT_CODE = arg0;
            }
            EXIT_SENTINEL
        }
        SYS_TICKS => crate::arch::timer::ticks(),
        _ => ENOSYS,
    }
}
