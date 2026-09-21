use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-changed=../userspace/recontrol/kernel_probe.ll");
    println!("cargo:rerun-if-env-changed=CLANG");
    println!("cargo:rerun-if-env-changed=LLD");
    println!("cargo:rerun-if-env-changed=GENERIC_GUI_ELF");

    let target = env::var("TARGET").expect("TARGET is set by Cargo");
    let user_init = if target == "x86_64-unknown-none" {
        Some(build_user_init())
    } else {
        None
    };
    let user_gui = env::var_os("GENERIC_GUI_ELF").map(PathBuf::from);
    if let Some(path) = user_gui.as_ref() {
        println!("cargo:rerun-if-changed={}", path.display());
        assert!(
            path.is_file(),
            "GENERIC_GUI_ELF does not point to a file: {}",
            path.display()
        );
    }
    build_initramfs(user_init.as_deref(), user_gui.as_deref());

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

fn build_user_init() -> PathBuf {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let source = manifest.join("../userspace/bootstrap/init.S");
    let linker_script = manifest.join("../userspace/bootstrap/user.ld");
    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", linker_script.display());

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let object = out.join("generic-init.o");
    let elf = out.join("generic-init.elf");
    let clang = env::var_os("CLANG").unwrap_or_else(|| "clang".into());
    let lld = env::var_os("LLD").unwrap_or_else(|| "ld.lld".into());

    let compile = Command::new(&clang)
        .arg("-target")
        .arg("x86_64-unknown-none")
        .arg("-ffreestanding")
        .arg("-fno-stack-protector")
        .arg("-mno-red-zone")
        .arg("-c")
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .status()
        .unwrap_or_else(|error| panic!("failed to execute {:?}: {error}", clang));
    assert!(compile.success(), "clang failed to compile userspace init");

    let link = Command::new(&lld)
        .arg("-m")
        .arg("elf_x86_64")
        .arg("-T")
        .arg(&linker_script)
        .arg("-o")
        .arg(&elf)
        .arg(&object)
        .status()
        .unwrap_or_else(|error| panic!("failed to execute {:?}: {error}", lld));
    assert!(link.success(), "ld.lld failed to link userspace init");
    elf
}

fn build_initramfs(user_init: Option<&Path>, user_gui: Option<&Path>) {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let root = manifest.join("../initramfs");
    println!("cargo:rerun-if-changed={}", root.display());

    let mut archive = Vec::from(&b"GIR1"[..]);
    if root.is_dir() {
        append_directory(&root, &root, &mut archive);
    }
    if let Some(user_init) = user_init {
        let data = fs::read(user_init).expect("read generated userspace init ELF");
        append_record(&mut archive, 1, "bin/init", &data);
    }
    if let Some(user_gui) = user_gui {
        let data = fs::read(user_gui).expect("read Generic GUI userspace ELF");
        append_record(&mut archive, 1, "bin/generic-gui", &data);
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
