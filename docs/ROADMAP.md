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
3. Interrupts and time: ACPI/MADT, APIC/IOAPIC, timer and monotonic clock.
4. Tasks: kernel threads, preemption, SMP, per-CPU state and synchronization.
5. Isolation: ring 3, separate address spaces, syscalls, user-pointer checking,
   ELF loader and init process.
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
