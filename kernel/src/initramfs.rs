use alloc::string::String;
use kernel_core::vfs::{NodeKind, Vfs, VfsError};

static ARCHIVE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/initramfs.gir"));

pub fn unpack(vfs: &mut Vfs) -> Result<usize, VfsError> {
    if ARCHIVE.len() < 5 || &ARCHIVE[..4] != b"GIR1" {
        return Err(VfsError::CorruptFilesystem);
    }

    let mut offset = 4usize;
    let mut files = 0usize;
    loop {
        let kind = *ARCHIVE.get(offset).ok_or(VfsError::CorruptFilesystem)?;
        offset += 1;
        if kind == 0 {
            break;
        }

        let path_len = read_u16(&mut offset)? as usize;
        let data_len = read_u32(&mut offset)? as usize;
        let path_bytes = ARCHIVE
            .get(offset..offset + path_len)
            .ok_or(VfsError::CorruptFilesystem)?;
        offset += path_len;
        let path_text =
            core::str::from_utf8(path_bytes).map_err(|_| VfsError::CorruptFilesystem)?;
        let mut path = String::from("/");
        path.push_str(path_text);

        match kind {
            1 => {
                let data = ARCHIVE
                    .get(offset..offset + data_len)
                    .ok_or(VfsError::CorruptFilesystem)?;
                offset += data_len;
                match vfs.metadata(&path) {
                    Ok(metadata) if metadata.kind == NodeKind::File => {
                        vfs.truncate(&path, 0)?;
                    }
                    Ok(_) => return Err(VfsError::AlreadyExists),
                    Err(VfsError::NotFound) => {
                        vfs.create_file(&path)?;
                    }
                    Err(error) => return Err(error),
                }
                vfs.write(&path, 0, data)?;
                files += 1;
            }
            2 => {
                if data_len != 0 {
                    return Err(VfsError::CorruptFilesystem);
                }
                match vfs.metadata(&path) {
                    Ok(metadata) if metadata.kind == NodeKind::Directory => {}
                    Ok(_) => return Err(VfsError::AlreadyExists),
                    Err(VfsError::NotFound) => {
                        vfs.create_dir(&path)?;
                    }
                    Err(error) => return Err(error),
                }
            }
            _ => return Err(VfsError::CorruptFilesystem),
        }
    }

    Ok(files)
}

fn read_u16(offset: &mut usize) -> Result<u16, VfsError> {
    let bytes = ARCHIVE
        .get(*offset..*offset + 2)
        .ok_or(VfsError::CorruptFilesystem)?;
    *offset += 2;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(offset: &mut usize) -> Result<u32, VfsError> {
    let bytes = ARCHIVE
        .get(*offset..*offset + 4)
        .ok_or(VfsError::CorruptFilesystem)?;
    *offset += 4;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}
