# Generic OS roadmap

1. Bootstrap — working now: UEFI to x86_64 ELF, serial diagnostics, GDT/TSS/IDT,
   RAM allocation test, returning breakpoint, freestanding Recontrol ABI call,
   x86_64 QEMU smoke and x128 reference smoke.
2. Protected memory: permanent PMM, owned page tables, NX/W^X, guard pages,
   kernel heap and complete fault coverage.
3. Interrupts and time: ACPI/MADT, APIC/IOAPIC, timer and monotonic clock.
4. Tasks: kernel threads, preemption, SMP, per-CPU state and synchronization.
5. Isolation: ring 3, separate address spaces, syscalls, user-pointer checking,
   ELF loader and init process.
6. Recontrol userspace runtime: no_std Generic runtime, stable syscall ABI,
   process exit, byte/string output, allocation and panic path.
7. Storage: VFS, initramfs/ramfs, virtio-blk and a persistent filesystem.
8. User environment: shell, utilities, IPC and system services.
9. Networking and hardware qualification: PCI, virtio-net, real devices,
   fuzzing and security review.
10. Native x128 port: define trap/privilege/MMU/atomic contracts, add a compiler
    backend and replace the reference bootstrap with a full Generic kernel build.

Every stage should end with an observable, reproducible acceptance test.
The x128 target remains experimental until its ABI and toolchain are stable.
