# Generic interactive console

Generic has an interactive text console rendered into the UEFI pixel
framebuffer. It does not depend on legacy VGA text mode. This is the preferred
path for VirtualBox and modern UEFI machines.

Keyboard input currently uses the PS/2/i8042 compatibility controller in
polling mode, so hardware IRQs do not need to be enabled yet. VirtualBox and
QEMU expose this compatibility interface.

Available commands:

- help
- clear
- echo TEXT
- uname
- version
- mem
- video
- recontrol
- whoami
- pwd
- ls
- reboot
- halt / shutdown

The shell is intentionally a kernel console for now. There are no user
processes, VFS, pipes, permissions or executable programs yet. As those
subsystems are added, this console can become the early userspace shell.

## QEMU

Build normally, then open a graphical window:

    bash scripts/build.sh
    python3 scripts/run.py --graphical

To test the same optical-disc path used by VirtualBox:

    bash scripts/build-iso.sh
    python3 scripts/run.py --iso --graphical

## VirtualBox ISO boot

Build the ISO:

    bash scripts/build-iso.sh

The result is:

    build/generic-uefi.iso

Create a 64-bit VM with EFI enabled, one CPU and at least 256 MiB of RAM.
You do not need a VDI or VHD for the current live kernel console.

Open the VM settings, attach build/generic-uefi.iso to the virtual optical
drive, make the optical drive bootable, and start the VM. The Generic
framebuffer console should appear directly in the VM display. Click inside the
window and type HELP.

The ISO is a UEFI El Torito image. The EFI boot image inside it is extracted
from the same EFI System Partition produced by the normal Generic disk-image
builder, so disk and optical boot use the same bootloader and kernel payload.

The x128 reference target is separate and is not supported by VirtualBox.
