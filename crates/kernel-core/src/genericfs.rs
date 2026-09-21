use crate::{
    block::{BlockDevice, BlockError, SECTOR_SIZE},
    vfs::{DirEntry, FileSystem, Inode, Metadata, NodeKind, VfsError},
};
use alloc::{boxed::Box, string::String, vec, vec::Vec};

const MAGIC: &[u8; 8] = b"GENFS1\0\0";
const VERSION: u32 = 1;
const INODE_COUNT: usize = 64;
const INODE_SIZE: usize = 128;
const INODES_PER_SECTOR: usize = SECTOR_SIZE / INODE_SIZE;
const INODE_TABLE_START: u64 = 1;
const INODE_TABLE_SECTORS: u64 =
    ((INODE_COUNT * INODE_SIZE + SECTOR_SIZE - 1) / SECTOR_SIZE) as u64;
const DATA_START: u64 = INODE_TABLE_START + INODE_TABLE_SECTORS;
const MAX_DISK_NAME: usize = 63;

#[derive(Clone, Copy)]
struct DiskInode {
    kind: u8,
    name_len: u8,
    parent: u64,
    len: u64,
    start_sector: u64,
    sector_count: u32,
    name: [u8; 64],
}

impl DiskInode {
    const fn empty() -> Self {
        Self {
            kind: 0,
            name_len: 0,
            parent: 0,
            len: 0,
            start_sector: 0,
            sector_count: 0,
            name: [0; 64],
        }
    }

    fn root() -> Self {
        Self {
            kind: 2,
            parent: 1,
            ..Self::empty()
        }
    }

    fn node_kind(self) -> Result<NodeKind, VfsError> {
        match self.kind {
            1 => Ok(NodeKind::File),
            2 => Ok(NodeKind::Directory),
            _ => Err(VfsError::NotFound),
        }
    }

    fn name(&self) -> Result<String, VfsError> {
        let len = self.name_len as usize;
        let text =
            core::str::from_utf8(&self.name[..len]).map_err(|_| VfsError::CorruptFilesystem)?;
        Ok(String::from(text))
    }
}

pub struct GenericFs {
    device: Box<dyn BlockDevice>,
}

impl GenericFs {
    pub fn mount(
        device: Box<dyn BlockDevice>,
        format_if_missing: bool,
    ) -> Result<(Self, bool), VfsError> {
        if device.sector_count() <= DATA_START + 1 {
            return Err(VfsError::NoSpace);
        }

        let fs = Self { device };
        let mut superblock = [0u8; SECTOR_SIZE];
        fs.device
            .read_sector(0, &mut superblock)
            .map_err(map_block)?;

        if &superblock[..8] == MAGIC {
            if read_u32(&superblock, 8) != VERSION
                || read_u32(&superblock, 12) as usize != INODE_COUNT
                || read_u64(&superblock, 16) != DATA_START
                || read_u64(&superblock, 24) != fs.device.sector_count()
            {
                return Err(VfsError::CorruptFilesystem);
            }
            if fs.read_inode(1)?.kind != 2 {
                return Err(VfsError::CorruptFilesystem);
            }
            return Ok((fs, false));
        }

        if !format_if_missing {
            return Err(VfsError::CorruptFilesystem);
        }
        fs.format()?;
        Ok((fs, true))
    }

    fn format(&self) -> Result<(), VfsError> {
        let zero = [0u8; SECTOR_SIZE];
        for sector in 0..DATA_START {
            self.device.write_sector(sector, &zero).map_err(map_block)?;
        }

        let mut superblock = [0u8; SECTOR_SIZE];
        superblock[..8].copy_from_slice(MAGIC);
        write_u32(&mut superblock, 8, VERSION);
        write_u32(&mut superblock, 12, INODE_COUNT as u32);
        write_u64(&mut superblock, 16, DATA_START);
        write_u64(&mut superblock, 24, self.device.sector_count());
        self.device
            .write_sector(0, &superblock)
            .map_err(map_block)?;
        self.write_inode(1, DiskInode::root())
    }

    fn read_inode(&self, inode: Inode) -> Result<DiskInode, VfsError> {
        let index = inode_index(inode)?;
        let sector = INODE_TABLE_START + (index / INODES_PER_SECTOR) as u64;
        let offset = (index % INODES_PER_SECTOR) * INODE_SIZE;
        let mut raw = [0u8; SECTOR_SIZE];
        self.device
            .read_sector(sector, &mut raw)
            .map_err(map_block)?;
        decode_inode(&raw[offset..offset + INODE_SIZE])
    }

    fn write_inode(&self, inode: Inode, value: DiskInode) -> Result<(), VfsError> {
        let index = inode_index(inode)?;
        let sector = INODE_TABLE_START + (index / INODES_PER_SECTOR) as u64;
        let offset = (index % INODES_PER_SECTOR) * INODE_SIZE;
        let mut raw = [0u8; SECTOR_SIZE];
        self.device
            .read_sector(sector, &mut raw)
            .map_err(map_block)?;
        encode_inode(&mut raw[offset..offset + INODE_SIZE], value);
        self.device.write_sector(sector, &raw).map_err(map_block)
    }

    fn find_child(&self, parent: Inode, name: &str) -> Result<Inode, VfsError> {
        for inode in 2..=INODE_COUNT as u64 {
            let entry = self.read_inode(inode)?;
            if entry.kind != 0 && entry.parent == parent && entry.name()? == name {
                return Ok(inode);
            }
        }
        Err(VfsError::NotFound)
    }

    fn allocate_inode(&self) -> Result<Inode, VfsError> {
        for inode in 2..=INODE_COUNT as u64 {
            if self.read_inode(inode)?.kind == 0 {
                return Ok(inode);
            }
        }
        Err(VfsError::NoSpace)
    }

    fn create(&self, parent: Inode, name: &str, kind: u8) -> Result<Inode, VfsError> {
        validate_disk_name(name)?;
        if self.read_inode(parent)?.node_kind()? != NodeKind::Directory {
            return Err(VfsError::NotDirectory);
        }
        if self.find_child(parent, name).is_ok() {
            return Err(VfsError::AlreadyExists);
        }

        let inode = self.allocate_inode()?;
        let mut entry = DiskInode::empty();
        entry.kind = kind;
        entry.parent = parent;
        entry.name_len = name.len() as u8;
        entry.name[..name.len()].copy_from_slice(name.as_bytes());
        self.write_inode(inode, entry)?;
        Ok(inode)
    }

    fn find_extent(&self, sectors: u64, exclude: Inode) -> Result<u64, VfsError> {
        if sectors == 0 {
            return Ok(0);
        }

        let mut used = Vec::new();
        for inode in 2..=INODE_COUNT as u64 {
            if inode == exclude {
                continue;
            }
            let entry = self.read_inode(inode)?;
            if entry.kind == 1 && entry.sector_count != 0 {
                used.push((
                    entry.start_sector,
                    entry.start_sector + entry.sector_count as u64,
                ));
            }
        }
        used.sort_unstable_by_key(|extent| extent.0);

        let mut candidate = DATA_START;
        for (start, end) in used {
            if candidate + sectors <= start {
                return Ok(candidate);
            }
            candidate = candidate.max(end);
        }
        if candidate + sectors <= self.device.sector_count() {
            Ok(candidate)
        } else {
            Err(VfsError::NoSpace)
        }
    }

    fn ensure_capacity(
        &self,
        inode: Inode,
        entry: &mut DiskInode,
        needed: u64,
    ) -> Result<(), VfsError> {
        if needed <= entry.sector_count as u64 {
            return Ok(());
        }

        let new_start = self.find_extent(needed, inode)?;
        let zero = [0u8; SECTOR_SIZE];
        for sector in 0..needed {
            self.device
                .write_sector(new_start + sector, &zero)
                .map_err(map_block)?;
        }
        for sector in 0..entry.sector_count as u64 {
            let mut buffer = [0u8; SECTOR_SIZE];
            self.device
                .read_sector(entry.start_sector + sector, &mut buffer)
                .map_err(map_block)?;
            self.device
                .write_sector(new_start + sector, &buffer)
                .map_err(map_block)?;
        }

        entry.start_sector = new_start;
        entry.sector_count = u32::try_from(needed).map_err(|_| VfsError::NoSpace)?;
        Ok(())
    }

    fn zero_range(&self, entry: &DiskInode, start: usize, end: usize) -> Result<(), VfsError> {
        if start >= end {
            return Ok(());
        }

        let mut position = start;
        while position < end {
            let sector_index = position / SECTOR_SIZE;
            let within = position % SECTOR_SIZE;
            let count = (SECTOR_SIZE - within).min(end - position);
            let mut buffer = [0u8; SECTOR_SIZE];
            self.device
                .read_sector(entry.start_sector + sector_index as u64, &mut buffer)
                .map_err(map_block)?;
            buffer[within..within + count].fill(0);
            self.device
                .write_sector(entry.start_sector + sector_index as u64, &buffer)
                .map_err(map_block)?;
            position += count;
        }
        Ok(())
    }
}

impl FileSystem for GenericFs {
    fn name(&self) -> &'static str {
        "genericfs"
    }

    fn root_inode(&self) -> Inode {
        1
    }

    fn metadata(&self, inode: Inode) -> Result<Metadata, VfsError> {
        let entry = self.read_inode(inode)?;
        Ok(Metadata {
            inode,
            kind: entry.node_kind()?,
            len: if entry.kind == 1 { entry.len } else { 0 },
        })
    }

    fn lookup(&self, parent: Inode, name: &str) -> Result<Inode, VfsError> {
        self.find_child(parent, name)
    }

    fn read_dir(&self, inode: Inode) -> Result<Vec<DirEntry>, VfsError> {
        if self.read_inode(inode)?.node_kind()? != NodeKind::Directory {
            return Err(VfsError::NotDirectory);
        }

        let mut entries = Vec::new();
        for child in 2..=INODE_COUNT as u64 {
            let entry = self.read_inode(child)?;
            if entry.kind != 0 && entry.parent == inode {
                entries.push(DirEntry {
                    name: entry.name()?,
                    metadata: self.metadata(child)?,
                });
            }
        }
        entries.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    fn create_file(&mut self, parent: Inode, name: &str) -> Result<Inode, VfsError> {
        self.create(parent, name, 1)
    }

    fn create_dir(&mut self, parent: Inode, name: &str) -> Result<Inode, VfsError> {
        self.create(parent, name, 2)
    }

    fn read(&self, inode: Inode, offset: usize, buffer: &mut [u8]) -> Result<usize, VfsError> {
        let entry = self.read_inode(inode)?;
        if entry.node_kind()? == NodeKind::Directory {
            return Err(VfsError::IsDirectory);
        }

        let len = usize::try_from(entry.len).map_err(|_| VfsError::OffsetOverflow)?;
        if offset >= len {
            return Ok(0);
        }

        let total = buffer.len().min(len - offset);
        let mut done = 0usize;
        while done < total {
            let position = offset + done;
            let sector_index = position / SECTOR_SIZE;
            let within = position % SECTOR_SIZE;
            let count = (SECTOR_SIZE - within).min(total - done);
            let mut sector = [0u8; SECTOR_SIZE];
            self.device
                .read_sector(entry.start_sector + sector_index as u64, &mut sector)
                .map_err(map_block)?;
            buffer[done..done + count].copy_from_slice(&sector[within..within + count]);
            done += count;
        }
        Ok(total)
    }

    fn write(&mut self, inode: Inode, offset: usize, data: &[u8]) -> Result<usize, VfsError> {
        let mut entry = self.read_inode(inode)?;
        if entry.node_kind()? == NodeKind::Directory {
            return Err(VfsError::IsDirectory);
        }

        let end = offset
            .checked_add(data.len())
            .ok_or(VfsError::OffsetOverflow)?;
        let needed = sectors_for_len(end);
        self.ensure_capacity(inode, &mut entry, needed)?;

        let old_len = usize::try_from(entry.len).map_err(|_| VfsError::OffsetOverflow)?;
        if offset > old_len {
            self.zero_range(&entry, old_len, offset)?;
        }

        let mut done = 0usize;
        while done < data.len() {
            let position = offset + done;
            let sector_index = position / SECTOR_SIZE;
            let within = position % SECTOR_SIZE;
            let count = (SECTOR_SIZE - within).min(data.len() - done);
            let mut sector = [0u8; SECTOR_SIZE];
            self.device
                .read_sector(entry.start_sector + sector_index as u64, &mut sector)
                .map_err(map_block)?;
            sector[within..within + count].copy_from_slice(&data[done..done + count]);
            self.device
                .write_sector(entry.start_sector + sector_index as u64, &sector)
                .map_err(map_block)?;
            done += count;
        }

        entry.len = entry.len.max(end as u64);
        self.write_inode(inode, entry)?;
        Ok(data.len())
    }

    fn truncate(&mut self, inode: Inode, len: usize) -> Result<(), VfsError> {
        let mut entry = self.read_inode(inode)?;
        if entry.node_kind()? == NodeKind::Directory {
            return Err(VfsError::IsDirectory);
        }

        self.ensure_capacity(inode, &mut entry, sectors_for_len(len))?;
        let old_len = usize::try_from(entry.len).map_err(|_| VfsError::OffsetOverflow)?;
        if len > old_len {
            self.zero_range(&entry, old_len, len)?;
        }
        entry.len = len as u64;
        self.write_inode(inode, entry)
    }

    fn remove(&mut self, parent: Inode, name: &str) -> Result<(), VfsError> {
        let inode = self.find_child(parent, name)?;
        let entry = self.read_inode(inode)?;
        if entry.kind == 2 {
            for child in 2..=INODE_COUNT as u64 {
                let candidate = self.read_inode(child)?;
                if candidate.kind != 0 && candidate.parent == inode {
                    return Err(VfsError::DirectoryNotEmpty);
                }
            }
        }
        self.write_inode(inode, DiskInode::empty())
    }
}

fn sectors_for_len(len: usize) -> u64 {
    if len == 0 {
        0
    } else {
        ((len + SECTOR_SIZE - 1) / SECTOR_SIZE) as u64
    }
}

fn inode_index(inode: Inode) -> Result<usize, VfsError> {
    let index = inode.checked_sub(1).ok_or(VfsError::NotFound)? as usize;
    if index >= INODE_COUNT {
        Err(VfsError::NotFound)
    } else {
        Ok(index)
    }
}

fn validate_disk_name(name: &str) -> Result<(), VfsError> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return Err(VfsError::InvalidName);
    }
    if name.len() > MAX_DISK_NAME {
        return Err(VfsError::NameTooLong);
    }
    Ok(())
}

fn decode_inode(raw: &[u8]) -> Result<DiskInode, VfsError> {
    let mut name = [0u8; 64];
    name.copy_from_slice(&raw[32..96]);
    let entry = DiskInode {
        kind: raw[0],
        name_len: raw[2],
        parent: read_u64(raw, 4),
        len: read_u64(raw, 12),
        start_sector: read_u64(raw, 20),
        sector_count: read_u32(raw, 28),
        name,
    };
    if entry.name_len as usize > MAX_DISK_NAME || entry.kind > 2 {
        return Err(VfsError::CorruptFilesystem);
    }
    Ok(entry)
}

fn encode_inode(raw: &mut [u8], entry: DiskInode) {
    raw.fill(0);
    raw[0] = entry.kind;
    raw[2] = entry.name_len;
    write_u64(raw, 4, entry.parent);
    write_u64(raw, 12, entry.len);
    write_u64(raw, 20, entry.start_sector);
    write_u32(raw, 28, entry.sector_count);
    raw[32..96].copy_from_slice(&entry.name);
}

fn map_block(error: BlockError) -> VfsError {
    match error {
        BlockError::OutOfRange => VfsError::CorruptFilesystem,
        BlockError::ReadOnly | BlockError::Io => VfsError::Io,
    }
}

fn read_u32(buffer: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(buffer[offset..offset + 4].try_into().unwrap())
}

fn write_u32(buffer: &mut [u8], offset: usize, value: u32) {
    buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn read_u64(buffer: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(buffer[offset..offset + 8].try_into().unwrap())
}

fn write_u64(buffer: &mut [u8], offset: usize, value: u64) {
    buffer[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::Vfs;
    use alloc::sync::Arc;
    use std::sync::Mutex;

    struct SharedMemoryBlock {
        sectors: u64,
        bytes: Arc<Mutex<Vec<u8>>>,
    }

    impl BlockDevice for SharedMemoryBlock {
        fn sector_count(&self) -> u64 {
            self.sectors
        }

        fn read_sector(
            &self,
            sector: u64,
            buffer: &mut [u8; SECTOR_SIZE],
        ) -> Result<(), BlockError> {
            if sector >= self.sectors {
                return Err(BlockError::OutOfRange);
            }
            let start = sector as usize * SECTOR_SIZE;
            buffer.copy_from_slice(&self.bytes.lock().unwrap()[start..start + SECTOR_SIZE]);
            Ok(())
        }

        fn write_sector(&self, sector: u64, buffer: &[u8; SECTOR_SIZE]) -> Result<(), BlockError> {
            if sector >= self.sectors {
                return Err(BlockError::OutOfRange);
            }
            let start = sector as usize * SECTOR_SIZE;
            self.bytes.lock().unwrap()[start..start + SECTOR_SIZE].copy_from_slice(buffer);
            Ok(())
        }
    }

    fn device() -> (Box<dyn BlockDevice>, Arc<Mutex<Vec<u8>>>) {
        let bytes = Arc::new(Mutex::new(vec![0u8; 256 * SECTOR_SIZE]));
        (
            Box::new(SharedMemoryBlock {
                sectors: 256,
                bytes: bytes.clone(),
            }),
            bytes,
        )
    }

    #[test]
    fn survives_remount_and_preserves_tree() {
        let (device, bytes) = device();
        let (fs, formatted) = GenericFs::mount(device, true).unwrap();
        assert!(formatted);
        let mut vfs = Vfs::new(Box::new(fs));
        vfs.create_dir("/etc").unwrap();
        vfs.create_file("/etc/config").unwrap();
        vfs.write("/etc/config", 0, b"persistent").unwrap();
        drop(vfs);

        let device: Box<dyn BlockDevice> = Box::new(SharedMemoryBlock {
            sectors: 256,
            bytes,
        });
        let (fs, formatted) = GenericFs::mount(device, false).unwrap();
        assert!(!formatted);
        let vfs = Vfs::new(Box::new(fs));
        assert_eq!(vfs.read_all("/etc/config").unwrap(), b"persistent");
    }

    #[test]
    fn reuses_space_after_delete() {
        let (device, _) = device();
        let (fs, _) = GenericFs::mount(device, true).unwrap();
        let mut vfs = Vfs::new(Box::new(fs));
        vfs.create_file("/a").unwrap();
        vfs.write("/a", 0, &vec![1u8; 4096]).unwrap();
        vfs.remove("/a").unwrap();
        vfs.create_file("/b").unwrap();
        vfs.write("/b", 0, &vec![2u8; 4096]).unwrap();
        assert_eq!(vfs.read_all("/b").unwrap(), vec![2u8; 4096]);
    }
}
