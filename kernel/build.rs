use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    build_initramfs();
    println!("cargo:rerun-if-changed=../userspace/recontrol/kernel_probe.ll");
    println!("cargo:rerun-if-env-changed=CLANG");

    let target = env::var("TARGET").expect("TARGET is set by Cargo");
    if target != "x86_64-unknown-none" {
        return;
    }

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let input = manifest.join("../userspace/recontrol/kernel_probe.ll");
    let output =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("recontrol-kernel-probe.o");
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

    assert!(
        status.success(),
        "clang failed to compile Recontrol kernel probe"
    );
    println!(
        "cargo:rustc-link-arg-bin=generic-kernel={}",
        output.display()
    );
}


fn build_initramfs() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let root = manifest.join("../initramfs");
    println!("cargo:rerun-if-changed={}", root.display());

    let mut archive = Vec::from(&b"GIR1"[..]);
    if root.is_dir() {
        append_directory(&root, &root, &mut archive);
    }
    archive.push(0);

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("initramfs.gir");
    fs::write(output, archive).expect("write initramfs archive");
}

fn append_directory(root: &Path, directory: &Path, archive: &mut Vec<u8>) {
    let mut entries: Vec<_> = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("initramfs directory entry").path())
        .collect();
    entries.sort();

    for path in entries {
        let relative = path.strip_prefix(root).expect("initramfs relative path");
        let name = relative
            .to_str()
            .unwrap_or_else(|| panic!("non-UTF8 initramfs path: {}", relative.display()))
            .replace('\\', "/");
        let metadata = fs::metadata(&path).expect("initramfs metadata");

        if metadata.is_dir() {
            append_record(archive, 2, &name, &[]);
            append_directory(root, &path, archive);
        } else if metadata.is_file() {
            let data = fs::read(&path).expect("read initramfs file");
            append_record(archive, 1, &name, &data);
        }
    }
}

fn append_record(archive: &mut Vec<u8>, kind: u8, path: &str, data: &[u8]) {
    let path_len = u16::try_from(path.len()).expect("initramfs path too long");
    let data_len = u32::try_from(data.len()).expect("initramfs file too large");
    archive.push(kind);
    archive.extend_from_slice(&path_len.to_le_bytes());
    archive.extend_from_slice(&data_len.to_le_bytes());
    archive.extend_from_slice(path.as_bytes());
    archive.extend_from_slice(data);
}
