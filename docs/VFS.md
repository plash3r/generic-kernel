# Generic VFS

Generic now has a filesystem-independent virtual filesystem namespace and an
in-memory root filesystem.

The design follows the useful separation visible in Linux VFS/namei and ramfs:
pathname handling is not implemented by each filesystem, and filesystem
backends operate on inode-like object identities through a common interface.
The implementation is independent and intentionally much smaller.

## Layers

### VFS namespace

`crates/kernel-core/src/vfs.rs` owns:

- absolute/relative pathname normalization;
- `.` and `..` handling;
- `NAME_MAX` and `PATH_MAX` validation;
- inode metadata and directory entries;
- the `FileSystem` backend trait;
- mount table management;
- longest-prefix mount routing;
- create, lookup, read, write, truncate, directory listing and remove.

The root filesystem is mounted at `/`. Additional filesystem implementations
can be mounted at existing directories without changing shell or future syscall
code.

### ramfs

`RamFs` is the first backend. It provides read-write directories and regular
files entirely in kernel memory.

Each object has a stable inode number. Directories map names to inode numbers;
file contents live in dynamically allocated byte vectors backed by the Generic
kernel heap.

ramfs is volatile: all content disappears on reboot.

## Initial tree

At boot Generic creates:

    /
    /README
    /bin/
    /dev/
    /etc/
      motd
    /home/
    /mnt/
    /tmp/

Before the shell starts, the kernel performs an acceptance check that creates a
temporary directory and file, writes and reads data, stats and lists it, then
removes both objects. Successful boot prints:

    [ok] VFS root=ramfs, 1 mount, pathname/create/read/write/remove

## Shell commands

The framebuffer console now exposes filesystem operations:

    pwd
    cd PATH
    ls [PATH]
    cat PATH
    touch PATH
    mkdir PATH
    write PATH TEXT
    append PATH TEXT
    rm PATH
    stat PATH
    mounts

Relative paths are resolved against the shell's current directory.

Examples:

    generic:/# mkdir home/demo
    generic:/# cd home/demo
    generic:/home/demo# write hello.txt Hello Generic
    generic:/home/demo# cat hello.txt
    Hello Generic
    generic:/home/demo# stat hello.txt

## Current limits

This VFS is an important namespace/storage abstraction, not persistent storage
yet. Generic still needs:

- an initramfs loader;
- a block-device layer;
- virtio-blk/AHCI/NVMe drivers;
- at least one persistent filesystem backend;
- permissions/UID/GID;
- symlinks and hard links;
- per-process file descriptor tables and open-file offsets;
- page cache and writeback for scalable disk I/O.

Those features can be added behind the current VFS interface instead of
hard-coding them into the shell.
