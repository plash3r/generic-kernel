use alloc::{boxed::Box, string::String, vec::Vec};
use kernel_core::{
    block::BlockDevice,
    genericfs::GenericFs,
    vfs::{
        normalize_path, DirEntry, Metadata, MountInfo, NodeKind, RamFs, Vfs, VfsError,
    },
};
use spin::Mutex;

static VFS: Mutex<Option<Vfs>> = Mutex::new(None);

pub fn init(block_device: Option<Box<dyn BlockDevice>>) {
    let mut vfs = Vfs::new(Box::new(RamFs::new()));

    for path in ["/bin", "/dev", "/etc", "/home", "/mnt", "/tmp"] {
        vfs.create_dir(path)
            .unwrap_or_else(|error| panic!("failed to create {path}: {error}"));
    }

    let initramfs_files =
        crate::initramfs::unpack(&mut vfs).expect("failed to unpack Generic initramfs");
    crate::log!("[ok] initramfs {} files loaded into root ramfs\n", initramfs_files);

    verify_ramfs(&mut vfs);

    if let Some(device) = block_device {
        let sectors = device.sector_count();
        let (filesystem, formatted) =
            GenericFs::mount(device, true).expect("failed to mount GenericFS");
        vfs.mount("/mnt", Box::new(filesystem))
            .expect("failed to mount GenericFS at /mnt");

        let marker = b"generic-persistent-v1";
        let recovered = match vfs.metadata("/mnt/.generic-persist") {
            Ok(metadata) => {
                assert_eq!(metadata.kind, NodeKind::File);
                let contents = vfs
                    .read_all("/mnt/.generic-persist")
                    .expect("failed to read persistent marker");
                assert_eq!(contents, marker, "persistent marker corrupted");
                true
            }
            Err(VfsError::NotFound) => {
                vfs.create_file("/mnt/.generic-persist")
                    .expect("failed to create persistent marker");
                vfs.write("/mnt/.generic-persist", 0, marker)
                    .expect("failed to write persistent marker");
                false
            }
            Err(error) => panic!("persistent marker lookup failed: {error}"),
        };

        if formatted {
            seed_file(
                &mut vfs,
                "/mnt/README",
                b"GenericFS persistent volume. Files here survive reboot.\n",
            );
        }

        if recovered {
            crate::log!(
                "[ok] GenericFS recovered persistent volume: {} sectors mounted at /mnt\n",
                sectors
            );
        } else {
            crate::log!(
                "[ok] GenericFS initialized persistent volume: {} sectors mounted at /mnt\n",
                sectors
            );
        }
    } else {
        crate::log!("[warn] no virtio block device; persistent /mnt not mounted\n");
    }

    crate::log!(
        "[ok] VFS root=ramfs, {} mount(s), pathname/create/read/write/remove\n",
        vfs.mounts().len()
    );

    let mut global = VFS.lock();
    assert!(global.is_none(), "VFS initialized twice");
    *global = Some(vfs);
}

pub fn canonicalize(cwd: &str, path: &str) -> Result<String, VfsError> {
    normalize_path(cwd, path)
}

pub fn metadata(path: &str) -> Result<Metadata, VfsError> {
    with_vfs(|vfs| vfs.metadata(path))
}

pub fn read_dir(path: &str) -> Result<Vec<DirEntry>, VfsError> {
    with_vfs(|vfs| vfs.read_dir(path))
}

pub fn read_file(path: &str) -> Result<Vec<u8>, VfsError> {
    with_vfs(|vfs| vfs.read_all(path))
}

pub fn write_file(path: &str, data: &[u8], append: bool) -> Result<usize, VfsError> {
    with_vfs(|vfs| {
        let offset = match vfs.metadata(path) {
            Ok(metadata) => {
                if metadata.kind == NodeKind::Directory {
                    return Err(VfsError::IsDirectory);
                }
                if append {
                    usize::try_from(metadata.len).map_err(|_| VfsError::OffsetOverflow)?
                } else {
                    vfs.truncate(path, 0)?;
                    0
                }
            }
            Err(VfsError::NotFound) => {
                vfs.create_file(path)?;
                0
            }
            Err(error) => return Err(error),
        };

        vfs.write(path, offset, data)
    })
}

pub fn touch(path: &str) -> Result<(), VfsError> {
    with_vfs(|vfs| match vfs.metadata(path) {
        Ok(metadata) if metadata.kind == NodeKind::File => Ok(()),
        Ok(_) => Err(VfsError::IsDirectory),
        Err(VfsError::NotFound) => {
            vfs.create_file(path)?;
            Ok(())
        }
        Err(error) => Err(error),
    })
}

pub fn mkdir(path: &str) -> Result<(), VfsError> {
    with_vfs(|vfs| {
        vfs.create_dir(path)?;
        Ok(())
    })
}

pub fn remove(path: &str) -> Result<(), VfsError> {
    with_vfs(|vfs| vfs.remove(path))
}

pub fn mounts() -> Vec<MountInfo> {
    let global = VFS.lock();
    global
        .as_ref()
        .expect("VFS used before initialization")
        .mounts()
}

fn verify_ramfs(vfs: &mut Vfs) {
    vfs.create_dir("/tmp/.vfs-smoke")
        .expect("VFS smoke mkdir failed");
    vfs.create_file("/tmp/.vfs-smoke/probe")
        .expect("VFS smoke create failed");
    vfs.write("/tmp/.vfs-smoke/probe", 0, b"generic-vfs")
        .expect("VFS smoke write failed");
    let data = vfs
        .read_all("/tmp/.vfs-smoke/probe")
        .expect("VFS smoke read failed");
    assert_eq!(data, b"generic-vfs", "VFS smoke readback mismatch");
    assert!(vfs
        .read_dir("/tmp/.vfs-smoke")
        .expect("VFS smoke readdir failed")
        .iter()
        .any(|entry| entry.name == "probe"));
    vfs.remove("/tmp/.vfs-smoke/probe")
        .expect("VFS smoke unlink failed");
    vfs.remove("/tmp/.vfs-smoke")
        .expect("VFS smoke rmdir failed");
}

fn seed_file(vfs: &mut Vfs, path: &str, data: &[u8]) {
    match vfs.metadata(path) {
        Ok(metadata) if metadata.kind == NodeKind::File => {
            vfs.truncate(path, 0)
                .unwrap_or_else(|error| panic!("failed to truncate {path}: {error}"));
        }
        Ok(_) => panic!("{path} exists but is not a file"),
        Err(VfsError::NotFound) => {
            vfs.create_file(path)
                .unwrap_or_else(|error| panic!("failed to create {path}: {error}"));
        }
        Err(error) => panic!("failed to inspect {path}: {error}"),
    }
    vfs.write(path, 0, data)
        .unwrap_or_else(|error| panic!("failed to seed {path}: {error}"));
}

fn with_vfs<T>(operation: impl FnOnce(&mut Vfs) -> Result<T, VfsError>) -> Result<T, VfsError> {
    let mut global = VFS.lock();
    let vfs = global.as_mut().expect("VFS used before initialization");
    operation(vfs)
}
