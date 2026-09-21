# Generic OS — kernel

This repository contains the kernel of Generic OS.

The primary hardware target is x86_64. Generic supports both legacy BIOS and
UEFI boot paths for the VirtualBox/QEMU ISO. The repository also contains an
experimental x128 reference architecture that can execute Generic's bootstrap
in a deterministic emulator.

Generic is intentionally its own kernel. Upstream Linux is used as a design
reference for subsystem boundaries and proven kernel architecture, while
Generic keeps a smaller Rust-first implementation. See docs/LINUX_REFERENCE.md.

## Working now

The x86_64 path provides a no_std Rust kernel with:

- BIOS and UEFI loading;
- COM1 diagnostics;
- GDT/TSS/IDT and selected exception handlers;
- permanent physical-memory management with page allocation, free and
  coalescing;
- x86_64 page-table mapping for kernel-owned virtual ranges;
- a mapped 2 MiB kernel heap that is writable, non-executable and surrounded by
  unmapped guard pages;
- heap allocation/deallocation smoke validation;
- framebuffer terminal and PS/2 keyboard input;
- generic VFS namespace with mount routing and a writable ramfs root;
- interactive file commands: cd/ls/cat/touch/mkdir/write/append/rm/stat/mounts;
- freestanding Recontrol ABI integration;
- hybrid ISO and disk boot smoke tests.

A successful memory bring-up includes diagnostics similar to:

    [ok] physical frames ..., allocate/free/coalesce
    [ok] PMM ... MiB managed in ... regions, ... MiB free
    [ok] kernel heap 2048 KiB @ 0x444400000000, 512 pages, RW+NX, guard pages

## Build a VirtualBox ISO

On Ubuntu / WSL2 install:

    sudo apt-get install build-essential clang pkg-config qemu-system-x86 ovmf python3 xorriso nasm

Then build:

    bash scripts/build-iso.sh

The result is:

    build/generic.iso

This ISO has two El Torito boot entries:

- legacy BIOS, through a small Generic CD chainloader that enters the existing
  BIOS boot path;
- UEFI, using the Generic EFI System Partition.

That means the same ISO is intended to boot in VirtualBox whether EFI is
enabled or disabled.

You can test both firmware paths in QEMU:

    python3 scripts/run.py --iso --graphical
    python3 scripts/run.py --iso --bios --graphical

CI boots the smoke ISO in both UEFI and legacy BIOS modes.

## VirtualBox

Create an x86_64 VM with one CPU and at least 256 MiB of RAM. A virtual hard
disk is not required for the current live console.

Attach:

    build/generic.iso

to the VM's optical drive and start it. EFI may be enabled or disabled. The
Generic framebuffer terminal should appear in the VM window. Click inside the
window and type HELP.

Do not use the older build/generic-uefi.iso from an earlier revision; that
image was UEFI-only.

## UEFI disk image

The original UEFI disk image path remains available:

    bash scripts/build.sh
    python3 scripts/run.py --graphical

It produces build/generic-uefi.img.

## Build and run x128

    bash scripts/x128.sh build
    bash scripts/x128.sh smoke

The encoded image is build/generic-x128.img.

## Recontrol regeneration

Use an installed compiler:

    bash scripts/recontrol.sh

Or point at a checkout:

    RECONTROL_ROOT=../recontrol-lang bash scripts/recontrol.sh

Or at a specific compiler executable:

    RCL=/path/to/rcl bash scripts/recontrol.sh

CI pins the Recontrol revision recorded in userspace/recontrol/REVISION and
checks that the generated LLVM IR is reproducible.

## Project layout

- kernel/ — freestanding x86_64 kernel.
- kernel/src/mm/ — Generic memory-management policy and kernel heap.
- kernel/src/arch/ — x86_64-specific descriptor tables, MMU, I/O and devices.
- crates/kernel-core/ — platform-independent PMM and VFS/filesystem abstractions.
- arch/x86/ — x86 bootstrap helpers used by optical boot.
- arch/x128/ — x128 bootstrap source.
- tools/x128.py — x128 assembler/reference emulator.
- tools/uefi_iso.py — builds the hybrid BIOS + UEFI ISO.
- userspace/recontrol/ — Recontrol source, generated IR and compiler revision.
- tools/image/ — BIOS and UEFI disk-image builder.
- scripts/ — build, ISO, QEMU, x128 and Recontrol commands.
- docs/ — architecture decisions, Linux reference notes, roadmap and validation.

## Current boundary

Generic now has executable boot paths, a permanent PMM and an early protected
kernel heap, but it is not yet a complete general-purpose desktop/server OS.

The protected-memory stage still needs Generic-owned top-level page tables,
full kernel-section W^X enforcement and richer fault coverage. Later stages add
APIC/IOAPIC and timekeeping, scheduler/SMP, ring 3, syscalls, user ELF loading,
persistent block storage/filesystems, mature input/graphics drivers and networking.
