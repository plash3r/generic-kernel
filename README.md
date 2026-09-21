# Generic OS — kernel

This repository contains the kernel of Generic OS.

The primary hardware target is x86_64. Generic supports both legacy BIOS and
UEFI boot paths for the VirtualBox/QEMU ISO. The repository also contains an
experimental x128 reference architecture that can execute Generic's bootstrap
in a deterministic emulator.

## Working now

The x86_64 path provides a no_std Rust kernel, BIOS/UEFI loading, COM1
diagnostics, GDT/TSS/IDT, selected CPU exception handlers, a bootstrap
physical-frame allocator, RAM write/read validation, a framebuffer terminal,
PS/2 keyboard input and QEMU smoke tests.

Recontrol is integrated at the kernel ABI boundary. A function compiled from
userspace/recontrol/kernel_probe.rcl is turned into freestanding LLVM code,
linked into the kernel ELF and called during boot.

## Build a VirtualBox ISO

On Ubuntu / WSL2 install:

    sudo apt-get install build-essential clang pkg-config qemu-system-x86 ovmf python3 xorriso

Then build:

    bash scripts/build-iso.sh

The result is:

    build/generic.iso

This ISO has two El Torito boot entries:

- legacy BIOS, using the Generic BIOS bootloader image;
- UEFI, using the Generic EFI System Partition.

That means the same ISO can boot in VirtualBox whether EFI is enabled or not.

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
- crates/kernel-core/ — platform-independent safe algorithms.
- arch/x128/ — x128 bootstrap source.
- tools/x128.py — x128 assembler/reference emulator.
- tools/uefi_iso.py — builds the hybrid BIOS + UEFI ISO.
- userspace/recontrol/ — Recontrol source, generated IR and compiler revision.
- tools/image/ — BIOS and UEFI disk-image builder.
- scripts/ — build, ISO, QEMU, x128 and Recontrol commands.
- docs/ — architecture decisions, roadmap and validation notes.

## Current boundary

Generic now has executable boot paths and automated acceptance checks, but it is
not yet a complete general-purpose desktop/server OS. It still needs its own VM
manager and heap, hardware IRQ/timer support, scheduler/SMP, ring 3, syscalls,
user ELF loading, VFS/storage, mature input/graphics drivers and networking.
