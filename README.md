# Generic OS — kernel

This repository contains the kernel of Generic OS.

The primary hardware target is x86_64 + UEFI. The repository also contains an
experimental x128 reference architecture that can already execute Generic's
bootstrap in a deterministic emulator.

## Working now

The x86_64 path provides a no_std Rust kernel, UEFI loading, COM1 diagnostics,
GDT/TSS/IDT, selected CPU exception handlers, a bootstrap physical-frame
allocator, RAM write/read validation and a QEMU smoke test.

Recontrol is integrated at the kernel ABI boundary. A function compiled from
userspace/recontrol/kernel_probe.rcl is turned into freestanding LLVM code,
linked into the kernel ELF and called during boot. A successful boot includes:

    GENERIC: boot
    [ok] GDT / TSS / IDT
    [ok] Recontrol freestanding ABI (128)
    ...
    GENERIC: READY

x128 is a Generic-defined experimental ISA, not an existing hardware
architecture. Its current reference machine has 16 128-bit registers, a
128-bit PC/address model, fixed-size instructions, byte-addressable memory,
branches, output and deterministic halt status. The x128 smoke boot checks
full-width arithmetic, branches and memory and requires:

    GENERIC x128: READY

See docs/X128.md and docs/RECONTROL.md.

## Build x86_64

On Ubuntu / WSL2 install:

    sudo apt-get install build-essential clang pkg-config qemu-system-x86 ovmf python3

Then run:

    cargo test --locked -p kernel-core
    bash scripts/build.sh smoke
    python3 scripts/run.py --smoke

For a normal run:

    bash scripts/build.sh
    python3 scripts/run.py

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
- userspace/recontrol/ — Recontrol source, generated IR and compiler revision.
- tools/image/ — UEFI disk image builder.
- scripts/ — build, QEMU, x128 and Recontrol commands.
- docs/ — architecture decisions, roadmap and validation notes.

## Current boundary

Generic now has executable boot paths and automated acceptance checks, but it is
not yet a complete general-purpose desktop/server OS. It still needs its own VM
manager and heap, hardware IRQ/timer support, scheduler/SMP, ring 3, syscalls,
user ELF loading, VFS/storage, input, graphics and networking. These remain
tracked in docs/ROADMAP.md.
