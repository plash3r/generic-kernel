# Recontrol integration in Generic

Generic now links Recontrol-generated machine code directly into the
freestanding x86_64 kernel. The source is
userspace/recontrol/kernel_probe.rcl. The compiler emits LLVM IR, and
kernel/build.rs compiles that IR for x86_64-unknown-none and links the object
into the kernel ELF.

During boot the Rust kernel calls rcl_generic_probe() and requires the value
128. The QEMU smoke test therefore covers an actual Recontrol-to-kernel ABI
call, not just source-code validation.

The hosted Recontrol runtime is intentionally not linked because it currently
uses Rust std, stdout/stderr and process exit. Kernel-side Recontrol code must
stay freestanding until Generic has its own no_std runtime and stable syscall
surface.

userspace/recontrol/REVISION pins the compiler commit used by CI. Regenerate
the checked-in IR with either:

    RCL=/path/to/rcl bash scripts/recontrol.sh

or:

    RECONTROL_ROOT=../recontrol-lang bash scripts/recontrol.sh

CI regenerates the IR with the pinned compiler and fails if it differs.
