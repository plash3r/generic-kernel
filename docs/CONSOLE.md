# Generic interactive console

Generic has an interrupt-driven text console rendered into the boot framebuffer.
It shares the same framebuffer and input platform as the graphical desktop.

Keyboard scancodes arrive through the i8042 PS/2 controller on IOAPIC-routed
IRQ1 and are decoded from a bounded kernel queue. The mouse uses IRQ12 for the
graphical desktop. F12 switches between the text console and the desktop without
rebooting.

The primary system namespace is `KERNEL`; use `kernel help` for status,
diagnostics, desktop switching, font settings, mounts and machine control.
Filesystem commands operate through Generic VFS and GenericFS when mounted.

Useful mode-switch commands:

    kernel desktop

or press F12.

## QEMU

UEFI:

    bash scripts/build-iso.sh
    python3 scripts/run.py --iso --graphical

Legacy BIOS:

    python3 scripts/run.py --iso --bios --graphical

Normal framebuffer boots start in the graphical desktop. Use F12 or the
desktop Console control to enter this text console.

## VirtualBox

Build:

    bash scripts/build-iso.sh

Attach `build/generic.iso` as the virtual optical disc. The same hybrid ISO has
legacy BIOS and UEFI El Torito boot entries.

The current GUI input path uses the PS/2/i8042 compatibility controller. USB HID
is a later hardware-qualification step. The x128 reference target is separate
and is not supported by VirtualBox.
