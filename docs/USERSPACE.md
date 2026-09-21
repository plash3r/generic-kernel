# Generic userspace ABI foundation

Generic enters x86_64 userspace through DPL3 code/data selectors and uses
interrupt vector `0x80` as the bootstrap syscall gate.

Current syscall numbers:

    0  exit(status)
    1  ticks()
    2  write(fd, user_ptr, len)
    3  display_info(user_ptr, size)
    4  display_present(xrgb8888_ptr, bytes)
    5  input_poll(user_ptr, size)

The syscall number is passed in `RAX`. Arguments are passed in `RDI`, `RSI`
and `RDX`; the return value is in `RAX`.

All pointer-bearing syscalls validate the complete range against the active
process page tables. Copyout additionally requires writable USER_ACCESSIBLE
pages.

`write` currently accepts stdout/stderr descriptors and writes to COM1.
`display_info` and `input_poll` use bounded copyout. `display_present`
validates a full XRGB8888 user backbuffer and performs the native framebuffer
conversion in the kernel without allocating a kernel-sized frame copy.

The generated `/bin/init` ELF remains the short syscall/process smoke probe.
A normal GUI-enabled build also packages `/bin/generic-gui`. It runs in a
separate private CR3 with a larger userspace stack and remains in ring 3 as the
interactive graphical shell.

This ABI is still bootstrap-level. Next work is task-owned process context,
blocking event waits, address-space teardown, VFS-backed descriptors, IPC and
shared memory.
