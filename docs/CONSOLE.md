# Generic interactive console

Generic has an interactive text console rendered into a pixel framebuffer.

Keyboard input currently uses the PS/2/i8042 compatibility controller in
polling mode. This keeps the early shell usable before hardware IRQ/APIC support
is enabled.

Available commands include help, clear, echo, uname, version, mem, video,
recontrol, whoami, pwd, ls, reboot and halt.

The shell is still a kernel console. User processes, a VFS, pipes, permissions
and executable programs are later roadmap stages.

## QEMU

UEFI:

    bash scripts/build-iso.sh
    python3 scripts/run.py --iso --graphical

Legacy BIOS:

    python3 scripts/run.py --iso --bios --graphical

## VirtualBox

Build:

    bash scripts/build-iso.sh

Attach build/generic.iso as the virtual optical disc.

The ISO contains both legacy BIOS and UEFI El Torito boot entries, so VirtualBox
can boot the same file with EFI either enabled or disabled. One CPU and 256 MiB
of RAM are sufficient for the current console.

The x128 reference target is separate and is not supported by VirtualBox.
