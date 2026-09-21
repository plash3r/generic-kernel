# KERNEL command

`KERNEL` is the primary system command for Generic's kernel console. Kernel
settings, kernel status and machine-control operations live below this single
command tree instead of being spread across unrelated top-level commands.

## Command tree

    kernel
    kernel help
    kernel status
    kernel diagnostics
    kernel version
    kernel memory
    kernel tasks
    kernel video
    kernel font ...
    kernel mounts
    kernel reboot
    kernel halt

Running `kernel` with no arguments is the same as `kernel help`.

### Status

    kernel status

prints a compact combined view of:

- kernel version and architecture;
- framebuffer resolution and terminal grid;
- active console font and glyph size;
- physical-memory and heap availability;
- mounted filesystems.

### Diagnostics

    kernel diagnostics

runs non-destructive runtime self-checks across the active kernel. The command
checks PMM/heap accounting, APIC/IOAPIC state, the advancing system timer,
kernel scheduler/context switching, PS/2 keyboard/mouse status, the live CR3 and supervisor write-protect bit, heap
mapping and guard pages, framebuffer/font geometry, VFS/initramfs access,
GenericFS persistence when mounted, and the Recontrol freestanding ABI.

Each check is reported as `[ok]`, `[warn]` or `[fail]`, followed by a
summary. A missing optional persistent GenericFS mount is a warning rather than
a failure. `kernel diag` is accepted as a short alias.

### Font settings

Font management is a kernel setting namespace:

    kernel font
    kernel font list
    kernel font set noto16
    kernel font set noto20
    kernel font set noto24
    kernel font set bold16
    kernel font set bold20
    kernel font load /mnt/fonts/custom.psf
    kernel font reset

See `docs/FONTS.md` for PSF2 loading details.

### Memory, scheduler and video

    kernel memory
    kernel tasks
    kernel video

These are read-only inspection commands for the current kernel state.
`kernel tasks` lists scheduler task IDs, names, states, sleep deadlines and
the cumulative context-switch count.

### Machine control

    kernel reboot
    kernel halt

These operations act directly on the running machine and do not depend on a
userspace service.

## Compatibility aliases

The older top-level commands `VERSION`, `MEM`, `VIDEO`, `FONT`,
`MOUNTS`, `REBOOT`, `HALT` and `SHUTDOWN` still work so existing
workflows are not broken, but they are no longer advertised by the main HELP
screen. New kernel-console functionality should be added as a KERNEL
subcommand.

The command tree is intentionally centralized so later settings such as logging,
scheduler diagnostics, device controls or boot configuration can gain their own
subcommands without growing the top-level shell namespace.
