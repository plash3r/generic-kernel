# Linux reference model for Generic

Generic is its own kernel. Linux is used as a design reference, not as a source
tree to copy.

The current reference baseline is upstream Linux v7.2. At the time this
baseline was recorded, v7.3-rc4 was the newer release-candidate tag, so Generic
uses v7.2 when comparing stable subsystem structure.

## What Generic borrows conceptually

### Early architecture setup

Reference:

- Linux: arch/x86/kernel/setup.c

Linux keeps architecture discovery and boot-specific work under arch/x86 while
feeding generic kernel subsystems with normalized state. Generic follows the
same boundary: x86 page-table mechanics, descriptor tables and hardware I/O
live under kernel/src/arch, while allocation policy belongs outside arch.

### Early physical memory

Reference:

- Linux: mm/memblock.c
- Linux: include/linux/memblock.h

Linux represents early physical memory as sorted regions, avoids allocating the
null page, performs aligned range allocation, and later transitions away from
the temporary early allocator.

Generic uses the same ideas with a much smaller contract. PhysicalMemory owns a
fixed-capacity set of normalized free ranges, supports aligned page allocation,
freeing and coalescing, and does not depend on the heap.

### Page tables and protection

Reference:

- Linux: arch/x86/mm/init_64.c

Linux keeps page-table construction architecture-specific and treats mapping
permissions as part of the memory-management contract. Generic therefore keeps
the active x86_64 mapper in arch/memory.rs and maps its kernel heap writable but
non-executable. One unmapped guard page is left on each side of the heap.

Generic now replaces the bootloader CR3 with a deep copy of the entire table
tree allocated by its permanent PMM. Existing leaf mappings and permissions
are retained. Bootloader frames remain reserved; they are not reclaimed until
their other boot-time consumers can be audited. Section-level W^X and removing
writable/executable aliases are separate remaining milestones.

### Permanent page allocator

Reference:

- Linux: mm/page_alloc.c

Linux transitions from early memblock state to a long-lived page allocator and
then builds higher-level allocators above it. Generic now has the same layering
in simplified form:

1. firmware/bootloader usable ranges;
2. permanent PhysicalMemory PMM;
3. x86_64 virtual mappings;
4. kernel heap allocator.

A buddy allocator, per-CPU caches and NUMA policy are intentionally deferred
until Generic actually needs their scalability.

### VFS and ramfs

References:

- Linux: include/linux/fs.h
- Linux: fs/namei.c
- Linux: fs/ramfs/inode.c

Linux separates pathname resolution and the VFS object model from individual
filesystem implementations. Generic follows that boundary with a smaller
`FileSystem` trait, inode-like IDs, a mount table and longest-prefix mount
routing. `RamFs` is only the first backend; pathname consumers do not depend
on its representation.

Generic intentionally does not implement Linux dcache, page cache, credentials,
security hooks or the full POSIX inode model yet. Those layers should be added
only as processes and persistent storage require them.

### IRQ and scheduling direction

References:

- Linux: kernel/irq/irqdesc.c
- Linux: kernel/sched/core.c

Linux separates generic interrupt descriptors and scheduler policy from
architecture-specific interrupt-controller and context-switch machinery.
Generic will follow that separation for the next stages: generic IRQ/task
objects first, APIC/IOAPIC and x86 context switching behind arch interfaces.

## Licensing boundary

The Linux files above are GPL-licensed. Generic uses their architecture and
publicly documented concepts as engineering references. Implementations in this
repository are written independently for Generic rather than copied from Linux.

## Deliberate Generic differences

Generic stays small enough to understand end-to-end. It does not currently
carry Linux legacy-driver compatibility, NUMA, memory hotplug, cgroups,
multiple scheduler classes, module ABI compatibility or dozens of filesystem
and architecture variants. Those features should only appear if Generic's own
goals require them.
