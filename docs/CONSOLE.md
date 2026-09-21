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

## VirtualBox

Use build/generic-uefi.img as the source disk. If your VirtualBox setup prefers
VDI, convert the raw image with VBoxManage and attach the resulting VDI.

Create a 64-bit VM with EFI enabled, one CPU and at least 256 MiB of RAM. The
Generic framebuffer console should appear directly in the VM display. Click
inside the VM window and type HELP.

The x128 reference target is separate and is not supported by VirtualBox.
