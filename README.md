# Generic OS — kernel

This repository contains the kernel of Generic OS.

The primary hardware target is x86_64 + UEFI. The repository also contains an
experimental x128 reference architecture that can already execute Generic's
bootstrap in a deterministic emulator.

## Working now

The x86_64 path provides a no_std Rust kernel, UEFI loading, COM1 diagnostics,
GDT/TSS/IDT, selected CPU exception handlers, a bootstrap physical-frame
allocator, RAM write/read validation, a framebuffer terminal, PS/2 keyboard
input and QEMU smoke tests.

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

See docs/X128.md, docs/RECONTROL.md and docs/CONSOLE.md.

## Build x86_64

On Ubuntu / WSL2 install:

    sudo apt-get install build-essential clang pkg-config qemu-system-x86 ovmf python3 xorriso

Then run:

    cargo test --locked -p kernel-core
    bash scripts/build.sh smoke
    python3 scripts/run.py --smoke

For a normal graphical run:

    bash scripts/build.sh
    python3 scripts/run.py --graphical

## Build a bootable ISO

Generic can be built as a UEFI El Torito optical-disc image:

    bash scripts/build-iso.sh

The result is:

    build/generic-uefi.iso

The ISO is intended for VirtualBox, QEMU and other UEFI-capable virtual
machines. QEMU can verify the same optical boot path with:

    python3 scripts/run.py --iso --graphical

CI also creates and boots a smoke ISO to ensure that the optical-disc path is
actually bootable.

## VirtualBox

Create a 64-bit virtual machine, enable EFI, assign one CPU and at least
256 MiB of RAM. Do not create or attach a virtual hard disk for the first boot.

Attach:

    build/generic-uefi.iso

to the VM's optical drive and start the machine. The Generic framebuffer
terminal should appear directly in the VirtualBox window. Click inside it and
type HELP.

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
- tools/uefi_iso.py — extracts the EFI System Partition and creates a UEFI ISO.
- userspace/recontrol/ — Recontrol source, generated IR and compiler revision.
- tools/image/ — UEFI disk image builder.
- scripts/ — build, ISO, QEMU, x128 and Recontrol commands.
- docs/ — architecture decisions, roadmap and validation notes.

## Current boundary

Generic now has executable boot paths and automated acceptance checks, but it is
not yet a complete general-purpose desktop/server OS. It still needs its own VM
manager and heap, hardware IRQ/timer support, scheduler/SMP, ring 3, syscalls,
user ELF loading, VFS/storage, input drivers beyond the bootstrap PS/2 path,
graphics APIs and networking. These remain tracked in docs/ROADMAP.md.
