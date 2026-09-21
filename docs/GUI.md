# Generic GUI Phase 1

Generic now contains the first kernel-mode graphical desktop foundation. This
stage deliberately stays inside the kernel so graphics, input and window-system
contracts can stabilize before processes, ring 3 and a userspace display server
exist.

## Boot and interrupt platform

The x86_64 path discovers interrupt-controller topology from ACPI:

1. The bootloader-provided RSDP is validated.
2. Generic walks RSDT/XSDT and validates SDT checksums.
3. MADT provides the local APIC, IOAPICs and ISA interrupt-source overrides.
4. Generic maps LAPIC/IOAPIC pages through its own uncached MMIO mappings.
5. The legacy 8259 PIC is masked.
6. IOAPIC redirection routes IRQ0, IRQ1 and IRQ12 to Generic IDT vectors.
7. A PIT channel-0 clock runs at 100 Hz and LAPIC EOI completes interrupts.

The first system clock is intentionally PIT-based. The LAPIC timer remains
masked for now; a later scheduler stage can calibrate and use LAPIC/TSC timers.

## Input

The i8042 controller is initialized after IOAPIC routing.

- keyboard scancodes arrive through IRQ1 into a bounded queue;
- mouse packets arrive through IRQ12 into a bounded event queue;
- PS/2 IntelliMouse wheel mode is detected when the device supports it;
- the desktop consumes relative mouse events without polling hardware ports;
- the text shell consumes the same interrupt-driven keyboard queue;
- F12 switches from the graphical desktop to the text console and back.

USB HID is not part of Phase 1.

## Graphics

`arch::graphics::Display` owns one 32-bit software backbuffer and presents it
to the boot framebuffer. The layer provides:

- RGB colors;
- points and rectangles;
- clipped rectangle fills and borders;
- line drawing;
- anti-aliased Noto Sans Mono text;
- framebuffer conversion for RGB, BGR, grayscale and bootloader-described
  channel positions;
- dirty-region presentation;
- framebuffer checksum support for smoke validation.

The kernel heap is 16 MiB so a 1280x800 32-bit backbuffer (~4 MiB) fits with
room for the VFS, filesystem metadata and the rest of the early kernel.

## Desktop and compositor

The first compositor is a software, kernel-mode window manager. It provides:

- desktop background, top bar and taskbar;
- mouse cursor;
- z-order and focus;
- title-bar dragging;
- close and minimize controls;
- taskbar restore buttons;
- a Generic start menu;
- Welcome, Kernel Monitor and Files windows;
- a live kernel monitor using PMM/heap, timer, APIC, mounts and Recontrol data;
- a root-directory view backed by Generic VFS.

The desktop starts automatically on a normal framebuffer boot. Use F12, the
Console taskbar button, or the start-menu Text Console item to enter the
framebuffer shell. From the shell use:

    kernel desktop

or F12 to return to the desktop.

## Validation

Smoke builds still run headlessly. Before QEMU exits successfully they:

1. initialize ACPI/xAPIC/IOAPIC and observe PIT ticks;
2. allocate and render through the software graphics backbuffer;
3. render a complete desktop/compositor frame;
4. copy the frame to the hardware framebuffer;
5. emit a non-zero deterministic-style checksum marker.

The run script requires both the interrupt/timer marker and the desktop
compositor marker on disk, persistent-storage, UEFI ISO and BIOS ISO smoke
paths.

## Boundary after Phase 1

This is a real interactive graphical shell, but it is still kernel mode. It is
not yet the final Generic desktop architecture.

The next major stage is process infrastructure:

- scheduler and kernel threads;
- per-process address spaces and ring 3;
- syscall ABI and user-pointer validation;
- ELF program loading and `/bin/init`;
- file-descriptor tables;
- IPC and shared-memory surfaces;
- moving the compositor/display server and applications out of the kernel.

Later hardware work should add USB HID and accelerated/modern display drivers.
