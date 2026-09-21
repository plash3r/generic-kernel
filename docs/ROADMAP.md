# Generic OS roadmap

1. Bootstrap — working now: BIOS/UEFI to x86_64 ELF, serial diagnostics,
   GDT/TSS/IDT, framebuffer console, PS/2 input, freestanding Recontrol ABI,
   hybrid ISO boot tests and x128 reference smoke.
2. Protected memory — in progress: permanent range-based PMM with free/coalesce,
   active x86_64 page-table mapper, RW+NX kernel heap and unmapped guard pages
   are working. Generic now owns the entire four-level table tree and CR3, with
   CR0.WP enabled and rollback tests for failed table cloning. Remaining:
   kernel-section W^X hardening, richer page-fault diagnostics and allocator
   stress tests.
3. Interrupts and time — working now: ACPI/MADT discovery, xAPIC/IOAPIC
   routing, IRQ-driven PS/2 keyboard/mouse input, a 100 Hz PIT system timer and
   monotonic tick/millisecond clock. Remaining: LAPIC/TSC timer calibration,
   richer clock sources and SMP interrupt routing.
4. Tasks — in progress: cooperative round-robin kernel threads with dedicated
   stacks, context switching, sleeping, timer-driven reschedule requests and
   scheduler diagnostics are working. Remaining: hard IRQ preemption, SMP,
   per-CPU run queues/state and synchronization.
5. Isolation — in progress: DPL3 GDT segments, TSS RSP0, USER_ACCESSIBLE
   RX/RW+NX pages, a validated int 0x80 syscall boundary, ELF64 parser/loader
   and a real /bin/init ELF launched from initramfs are working. Each process
   now gets a private CR3 and a deep-cloned private page-table tree; user
   mappings never modify the kernel page-table tree. This conservative model
   works with the bootloader's existing low-half mappings. A process table
   tracks CR3, entry point and lifecycle, and /bin/init now runs as a
   scheduler-owned task with ready/running/exited transitions. Remaining:
   preemptive multi-process scheduling, address-space teardown/refcounting, a
   more memory-efficient shared supervisor kernel half and a long-lived
   init/userspace runtime.
   The first bounded user-pointer validation and stdout/stderr write syscall
   are working; this must expand into general copyin/copyout and FD/VFS APIs.
6. Recontrol userspace runtime: no_std Generic runtime, stable syscall ABI,
   process exit, byte/string output, allocation and panic path.
7. Storage — working foundation: generic VFS, generated initramfs, block-device
   API, x86_64 legacy virtio-blk and persistent GenericFS mounted at /mnt.
   Remaining for production storage: interrupt-driven I/O, AHCI/NVMe or
   VirtIO-SCSI, journaling/fsck, permissions and page cache/writeback.
8. User environment: shell, utilities, IPC and system services.
9. Networking and hardware qualification: PCI, virtio-net, real devices,
   fuzzing and security review.
10. Native x128 port: define trap/privilege/MMU/atomic contracts, add a compiler
    backend and replace the reference bootstrap with a full Generic kernel build.

Every stage must end with an observable, reproducible acceptance test.
The x128 target remains experimental until its ABI and toolchain are stable.

Generic follows Linux where the architecture is proven useful: generic policy
is kept separate from architecture-specific mechanism, early boot state is
transitioned into long-lived subsystems, and each subsystem exposes narrow
interfaces. Generic deliberately does not inherit Linux compatibility layers
that are not required by its own design.

Graphical-shell and compositor development is maintained separately in
`plash3r/generic-gui`. Generic kernel will expose the process, syscall, IPC,
shared-memory, framebuffer/display and input contracts that the GUI consumes.
