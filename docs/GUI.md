# Generic GUI userspace bridge

The Generic desktop, compositor and window manager live in the separate
`plash3r/generic-gui` repository. The kernel only provides privileged display,
input, process and memory mechanisms.

## ABI v1

The first working bridge uses three ring 3 syscalls in addition to the existing
bootstrap ABI:

    3  display_info(user_ptr, size)
    4  display_present(xrgb8888_ptr, bytes)
    5  input_poll(user_ptr, size)

`display_info` copies a fixed 32-byte structure to a validated writable
userspace buffer. The source format for presentation is XRGB8888.

`display_present` validates the complete userspace backbuffer range and then
converts/blits it into the boot framebuffer. The framebuffer MMIO mapping is
never made user-accessible.

`input_poll` copies a normalized 24-byte keyboard or mouse packet to userspace.
The GUI adapter converts those packets into `generic-gui-core::InputEvent`.

This full-frame, polling transport is deliberately simple. The next transport
stage is shared-memory surfaces, dirty rectangles and blocking event waits.

## Boot integration

A normal build automatically looks for `generic-gui` beside this repository or
at `.deps/generic-gui`. It builds `generic-gui-user`, packages the ELF into
initramfs as `/bin/generic-gui`, and the kernel launches it in a private CR3
after the bootstrap `/bin/init` probe.

If no GUI checkout is available, Generic falls back to the kernel framebuffer
console.
