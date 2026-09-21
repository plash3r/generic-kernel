extern "C" {
    fn rcl_generic_probe() -> i32;
}

/// Calls code produced by the Recontrol compiler and linked directly into the
/// freestanding kernel ELF. The probe deliberately avoids the hosted Recontrol
/// runtime (std, stdout, process exit), so the ABI boundary is kernel-safe.
pub fn probe() -> i32 {
    // SAFETY: kernel/build.rs links a freestanding object that exports exactly
    // extern C i32 rcl_generic_probe(void) from checked-in Recontrol LLVM IR.
    unsafe { rcl_generic_probe() }
}

pub fn verify() {
    let value = probe();
    assert_eq!(
        value, 128,
        "Recontrol kernel ABI probe returned the wrong value"
    );
    crate::log!("[ok] Recontrol freestanding ABI ({value})\n");
}
