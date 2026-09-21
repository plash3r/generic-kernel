# Generic interactive console

Generic kernel provides an interrupt-driven text console rendered into the boot
framebuffer.

Keyboard scancodes arrive through the i8042 PS/2 controller on IOAPIC-routed
IRQ1 and are decoded from a bounded kernel queue. Mouse events remain available
as a kernel input mechanism for future userspace consumers, but the kernel no
longer contains a desktop or window manager.

The primary system namespace is `KERNEL`; use `kernel help` for status,
diagnostics, font settings, mounts and machine control. Filesystem commands
operate through Generic VFS and GenericFS when mounted.

## QEMU

UEFI:

    bash scripts/build-iso.sh
    python3 scripts/run.py --iso --graphical

Legacy BIOS:

    python3 scripts/run.py --iso --bios --graphical

## VirtualBox

Build:

    bash scripts/build-iso.sh

Attach `build/generic.iso` as the virtual optical disc. The same hybrid ISO has
legacy BIOS and UEFI El Torito boot entries.

The graphical shell is developed in the separate `plash3r/generic-gui`
repository and will connect to Generic through userspace/syscall/IPC interfaces
rather than being linked into the kernel.
