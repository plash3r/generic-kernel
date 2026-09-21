# Generic storage stack

Generic now has a layered storage path:

1. kernel_core::block::BlockDevice — synchronous 512-byte sector interface.
2. x86_64 legacy virtio-blk PCI driver — DMA-backed queue with polling completion.
3. GenericFS v1 — persistent filesystem backend implementing the VFS FileSystem trait.
4. VFS mount table — GenericFS is mounted at /mnt when a supported block device exists.
5. initramfs — build-generated read-only boot payload unpacked into the root ramfs.

## GenericFS v1

GenericFS is intentionally small and independent of the device driver. The
on-disk layout uses:

- sector 0: superblock and format/version metadata;
- fixed inode table;
- contiguous file extents in the remaining sectors.

Directories are represented by parent inode IDs and names. Regular files store
a byte length plus their contiguous data extent. Removing a file releases its
extent implicitly because free-space discovery scans live inode extents.

This first format prioritizes correctness and inspectability over scalability.
It does not yet provide journaling, checksums, permissions, timestamps or crash
recovery beyond validating the superblock and inode records.

## virtio-blk

The x86_64 driver discovers the transitional virtio block PCI device through
PCI configuration mechanism #1. It negotiates the mandatory legacy feature
set, allocates virtqueue and request buffers directly from the PMM as DMA
pages, and issues synchronous single-sector requests.

Polling is deliberate for the current kernel stage: Generic does not yet have
the APIC/IOAPIC interrupt routing required for a proper interrupt-driven block
driver. The BlockDevice interface does not expose that implementation detail,
so the driver can move to interrupts later.

## initramfs

kernel/build.rs walks the initramfs directory and generates a compact GIR1
archive at build time. The kernel unpacks this archive into the root ramfs
before persistent storage is mounted.

The current image seeds /etc, /bin and boot documentation. This is the path
that will later carry the first user-mode init binary once ELF/ring3 support is
available.

## Running with persistent storage

Create a 16 MiB data disk:

    bash scripts/storage.sh create

Then boot with it attached:

    python3 scripts/run.py --storage --graphical

Files written below /mnt are stored on build/generic-storage.img and survive VM
restarts. Root ramfs files outside /mnt are rebuilt from initramfs every boot.

To erase the volume:

    bash scripts/storage.sh reset 16M

## Acceptance test

CI boots the same kernel twice with the same storage image:

    bash scripts/storage.sh reset 16M
    python3 scripts/run.py --smoke --storage
    python3 scripts/run.py --smoke --storage --expect-storage-recovered

The first boot formats GenericFS and writes a persistent marker. The second boot
must read the same marker and log:

    [ok] GenericFS recovered persistent volume

This detects regressions that a same-boot write/read smoke test would miss.

## Current limits

- the driver is the legacy transitional virtio-blk transport used by QEMU;
- I/O is synchronous and polling;
- GenericFS uses a fixed 64-inode table and contiguous extents;
- no journal, fsck, permissions, timestamps, symlinks or hard links yet;
- no page cache/writeback layer yet;
- AHCI/NVMe and VirtIO-SCSI remain future device backends.

The VFS and BlockDevice boundaries are already independent of these limits.
