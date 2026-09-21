# Graphical shell separation

The Generic kernel repository no longer contains the desktop, compositor or
window manager. Those components are developed in the separate
`plash3r/generic-gui` repository.

The kernel remains responsible for low-level mechanisms required by a graphical
environment:

- interrupt and timer infrastructure;
- keyboard and mouse input events;
- framebuffer/console ownership during early boot;
- memory management;
- processes and ring 3 (planned);
- syscalls and user-pointer validation (planned);
- IPC and shared memory (planned);
- display/input handoff interfaces for userspace (planned).

The GUI project is responsible for rendering policy, widgets, windows, desktop
behavior, applications and the eventual userspace display server.

This separation prevents desktop code from becoming part of the trusted kernel
surface and lets Generic GUI evolve independently from the kernel ABI.
