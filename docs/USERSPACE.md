# Generic userspace ABI foundation

Generic currently enters x86_64 userspace through DPL3 code/data selectors and
uses interrupt vector `0x80` as the bootstrap syscall gate. This is an early
ABI and is intentionally small while process scheduling and file-descriptor
semantics are still being built.

Current syscall numbers:

    0  exit(status)
    1  ticks()
    2  write(fd, user_ptr, len)

The syscall number is passed in `RAX`. Arguments are passed in `RDI`, `RSI`
and `RDX`; the return value is in `RAX`.

`write` currently accepts the process's stdout/stderr descriptors (1 and 2)
and writes bytes to the kernel serial console. Before dereferencing the pointer,
the kernel checks the full range against the active process page tables. Every
page-table level must be present and user-accessible. The syscall is bounded to
4096 bytes per call.

The generated `/bin/init` ELF exercises this boundary by writing a startup
message from its RX userspace segment, then reading the monotonic tick counter
and exiting with that value.

This is not yet a stable public ABI. Next work is scheduler-managed process
lifetime, copyout, VFS-backed descriptors, IPC and shared memory.
