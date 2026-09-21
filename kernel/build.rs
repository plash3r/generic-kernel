use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=../userspace/recontrol/kernel_probe.ll");
    println!("cargo:rerun-if-env-changed=CLANG");

    let target = env::var("TARGET").expect("TARGET is set by Cargo");
    if target != "x86_64-unknown-none" {
        return;
    }

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let input = manifest.join("../userspace/recontrol/kernel_probe.ll");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"))
        .join("recontrol-kernel-probe.o");
    let clang = env::var_os("CLANG").unwrap_or_else(|| "clang".into());

    let status = Command::new(&clang)
        .arg("-target")
        .arg("x86_64-unknown-none")
        .arg("-ffreestanding")
        .arg("-fno-stack-protector")
        .arg("-mno-red-zone")
        .arg("-Wno-override-module")
        .arg("-x")
        .arg("ir")
        .arg("-c")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .status()
        .unwrap_or_else(|error| panic!("failed to execute {:?}: {error}", clang));

    assert!(status.success(), "clang failed to compile Recontrol kernel probe");
    println!("cargo:rustc-link-arg-bin=generic-kernel={}", output.display());
}
