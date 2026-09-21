use alloc::{
    boxed::Box,
    collections::BTreeMap,
    string::{String, ToString},
    vec,
    vec::Vec,
};
use core::fmt;

pub const NAME_MAX: usize = 255;
pub const PATH_MAX: usize = 4096;
pub type Inode = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Metadata {
    pub inode: Inode,
    pub kind: NodeKind,
    pub len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub metadata: Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountInfo {
    pub path: String,
    pub filesystem: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VfsError {
    InvalidPath,
    InvalidName,
    NameTooLong,
    NotFound,
    AlreadyExists,
    NotDirectory,
    IsDirectory,
    DirectoryNotEmpty,
    RootImmutable,
    AlreadyMounted,
    MountPointNotDirectory,
    OffsetOverflow,
    InodeExhausted,
    Io,
    NoSpace,
    CorruptFilesystem,
}

impl fmt::Display for VfsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::InvalidPath => "invalid path",
            Self::InvalidName => "invalid file name",
            Self::NameTooLong => "name or path too long",
            Self::NotFound => "no such file or directory",
            Self::AlreadyExists => "file or directory already exists",
            Self::NotDirectory => "not a directory",
            Self::IsDirectory => "is a directory",
            Self::DirectoryNotEmpty => "directory not empty",
            Self::RootImmutable => "cannot remove filesystem root",
            Self::AlreadyMounted => "mount point already has a filesystem",
            Self::MountPointNotDirectory => "mount point is not a directory",
            Self::OffsetOverflow => "file offset overflow",
            Self::InodeExhausted => "inode space exhausted",
            Self::Io => "filesystem I/O error",
            Self::NoSpace => "no space left on device",
            Self::CorruptFilesystem => "corrupt or unsupported filesystem",
        };
        formatter.write_str(text)
    }
}

/// Filesystem backend contract used by the Generic VFS.
///
/// Pathname parsing and mount routing stay in Vfs. Backends only operate on
/// inode-like object identities and directory child names.
pub trait FileSystem: Send {
    fn name(&self) -> &'static str;
    fn root_inode(&self) -> Inode;
    fn metadata(&self, inode: Inode) -> Result<Metadata, VfsError>;
    fn lookup(&self, parent: Inode, name: &str) -> Result<Inode, VfsError>;
    fn read_dir(&self, inode: Inode) -> Result<Vec<DirEntry>, VfsError>;
    fn create_file(&mut self, parent: Inode, name: &str) -> Result<Inode, VfsError>;
    fn create_dir(&mut self, parent: Inode, name: &str) -> Result<Inode, VfsError>;
    fn read(&self, inode: Inode, offset: usize, buffer: &mut [u8]) -> Result<usize, VfsError>;
    fn write(&mut self, inode: Inode, offset: usize, data: &[u8]) -> Result<usize, VfsError>;
    fn truncate(&mut self, inode: Inode, len: usize) -> Result<(), VfsError>;
    fn remove(&mut self, parent: Inode, name: &str) -> Result<(), VfsError>;
}

struct Mount {
    path: String,
    filesystem: Box<dyn FileSystem>,
}

/// Generic pathname and mount layer.
///
/// The root filesystem is always mounted at /. Additional filesystems are
/// routed using longest-prefix mount matching.
pub struct Vfs {
    mounts: Vec<Mount>,
}

impl Vfs {
    pub fn new(root: Box<dyn FileSystem>) -> Self {
        Self {
            mounts: vec![Mount {
                path: "/".to_string(),
                filesystem: root,
            }],
        }
    }

    pub fn mount(&mut self, path: &str, filesystem: Box<dyn FileSystem>) -> Result<(), VfsError> {
        let path = normalize_path("/", path)?;
        if path == "/" || self.mounts.iter().any(|mount| mount.path == path) {
            return Err(VfsError::AlreadyMounted);
        }

        let metadata = self.metadata(&path)?;
        if metadata.kind != NodeKind::Directory {
            return Err(VfsError::MountPointNotDirectory);
        }

        self.mounts.push(Mount { path, filesystem });
        Ok(())
    }

    pub fn mounts(&self) -> Vec<MountInfo> {
        let mut mounts: Vec<_> = self
            .mounts
            .iter()
            .map(|mount| MountInfo {
                path: mount.path.clone(),
                filesystem: mount.filesystem.name(),
            })
            .collect();
        mounts.sort_unstable_by(|left, right| left.path.cmp(&right.path));
        mounts
    }

    pub fn metadata(&self, path: &str) -> Result<Metadata, VfsError> {
        let path = normalize_path("/", path)?;
        let (mount_index, relative) = self.route(&path);
        let filesystem = &*self.mounts[mount_index].filesystem;
        let inode = resolve_inode(filesystem, &relative)?;
        filesystem.metadata(inode)
    }

    pub fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, VfsError> {
        let path = normalize_path("/", path)?;
        let (mount_index, relative) = self.route(&path);
        let filesystem = &*self.mounts[mount_index].filesystem;
        let inode = resolve_inode(filesystem, &relative)?;
        filesystem.read_dir(inode)
    }

    pub fn create_file(&mut self, path: &str) -> Result<Metadata, VfsError> {
        let path = normalize_path("/", path)?;
        let (mount_index, relative) = self.route(&path);
        let filesystem = &mut *self.mounts[mount_index].filesystem;
        let (parent, name) = resolve_parent(filesystem, &relative)?;
        let inode = filesystem.create_file(parent, &name)?;
        filesystem.metadata(inode)
    }

    pub fn create_dir(&mut self, path: &str) -> Result<Metadata, VfsError> {
        let path = normalize_path("/", path)?;
        let (mount_index, relative) = self.route(&path);
        let filesystem = &mut *self.mounts[mount_index].filesystem;
        let (parent, name) = resolve_parent(filesystem, &relative)?;
        let inode = filesystem.create_dir(parent, &name)?;
        filesystem.metadata(inode)
    }

    pub fn remove(&mut self, path: &str) -> Result<(), VfsError> {
        let path = normalize_path("/", path)?;
        let (mount_index, relative) = self.route(&path);
        let filesystem = &mut *self.mounts[mount_index].filesystem;
        let (parent, name) = resolve_parent(filesystem, &relative)?;
        filesystem.remove(parent, &name)
    }

    pub fn read(&self, path: &str, offset: usize, buffer: &mut [u8]) -> Result<usize, VfsError> {
        let path = normalize_path("/", path)?;
        let (mount_index, relative) = self.route(&path);
        let filesystem = &*self.mounts[mount_index].filesystem;
        let inode = resolve_inode(filesystem, &relative)?;
        filesystem.read(inode, offset, buffer)
    }

    pub fn read_all(&self, path: &str) -> Result<Vec<u8>, VfsError> {
        let metadata = self.metadata(path)?;
        if metadata.kind == NodeKind::Directory {
            return Err(VfsError::IsDirectory);
        }

        let len = usize::try_from(metadata.len).map_err(|_| VfsError::OffsetOverflow)?;
        let mut data = vec![0u8; len];
        let read = self.read(path, 0, &mut data)?;
        data.truncate(read);
        Ok(data)
    }

    pub fn write(&mut self, path: &str, offset: usize, data: &[u8]) -> Result<usize, VfsError> {
        let path = normalize_path("/", path)?;
        let (mount_index, relative) = self.route(&path);
        let filesystem = &mut *self.mounts[mount_index].filesystem;
        let inode = resolve_inode(filesystem, &relative)?;
        filesystem.write(inode, offset, data)
    }

    pub fn truncate(&mut self, path: &str, len: usize) -> Result<(), VfsError> {
        let path = normalize_path("/", path)?;
        let (mount_index, relative) = self.route(&path);
        let filesystem = &mut *self.mounts[mount_index].filesystem;
        let inode = resolve_inode(filesystem, &relative)?;
        filesystem.truncate(inode, len)
    }

    fn route(&self, path: &str) -> (usize, String) {
        let mut best_index = 0usize;
        let mut best_len = 1usize;

        for (index, mount) in self.mounts.iter().enumerate().skip(1) {
            let exact = path == mount.path;
            let nested = path
                .strip_prefix(&mount.path)
                .is_some_and(|rest| rest.starts_with('/'));
            if (exact || nested) && mount.path.len() > best_len {
                best_index = index;
                best_len = mount.path.len();
            }
        }

        let mount_path = &self.mounts[best_index].path;
        if mount_path == "/" {
            return (best_index, path.to_string());
        }

        let rest = &path[mount_path.len()..];
        (
            best_index,
            if rest.is_empty() {
                "/".to_string()
            } else {
                rest.to_string()
            },
        )
    }
}

pub fn normalize_path(base: &str, path: &str) -> Result<String, VfsError> {
    if path.is_empty() || path.as_bytes().contains(&0) {
        return Err(VfsError::InvalidPath);
    }
    if !base.starts_with('/') || base.as_bytes().contains(&0) {
        return Err(VfsError::InvalidPath);
    }

    let mut components: Vec<String> = Vec::new();
    if !path.starts_with('/') {
        push_components(&mut components, base)?;
    }
    apply_components(&mut components, path)?;

    let mut normalized = String::from("/");
    for (index, component) in components.iter().enumerate() {
        if index > 0 {
            normalized.push('/');
        }
        normalized.push_str(component);
    }

    if normalized.len() > PATH_MAX {
        return Err(VfsError::NameTooLong);
    }
    Ok(normalized)
}

fn push_components(components: &mut Vec<String>, path: &str) -> Result<(), VfsError> {
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            name => {
                validate_name(name)?;
                components.push(name.to_string());
            }
        }
    }
    Ok(())
}

fn apply_components(components: &mut Vec<String>, path: &str) -> Result<(), VfsError> {
    push_components(components, path)
}

fn validate_name(name: &str) -> Result<(), VfsError> {
    if name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0') {
        return Err(VfsError::InvalidName);
    }
    if name.len() > NAME_MAX {
        return Err(VfsError::NameTooLong);
    }
    Ok(())
}

fn resolve_inode(filesystem: &dyn FileSystem, path: &str) -> Result<Inode, VfsError> {
    let mut inode = filesystem.root_inode();
    if path == "/" {
        return Ok(inode);
    }

    for component in path.trim_start_matches('/').split('/') {
        inode = filesystem.lookup(inode, component)?;
    }
    Ok(inode)
}

fn resolve_parent(filesystem: &dyn FileSystem, path: &str) -> Result<(Inode, String), VfsError> {
    if path == "/" {
        return Err(VfsError::RootImmutable);
    }

    let (parent_path, name) = path.rsplit_once('/').ok_or(VfsError::InvalidPath)?;
    validate_name(name)?;
    let parent_path = if parent_path.is_empty() {
        "/"
    } else {
        parent_path
    };
    let parent = resolve_inode(filesystem, parent_path)?;
    let metadata = filesystem.metadata(parent)?;
    if metadata.kind != NodeKind::Directory {
        return Err(VfsError::NotDirectory);
    }
    Ok((parent, name.to_string()))
}

struct RamNode {
    kind: NodeKind,
    children: BTreeMap<String, Inode>,
    data: Vec<u8>,
}

impl RamNode {
    fn directory() -> Self {
        Self {
            kind: NodeKind::Directory,
            children: BTreeMap::new(),
            data: Vec::new(),
        }
    }

    fn file() -> Self {
        Self {
            kind: NodeKind::File,
            children: BTreeMap::new(),
            data: Vec::new(),
        }
    }
}

/// Simple read-write memory filesystem used as Generic's first VFS backend.
pub struct RamFs {
    nodes: BTreeMap<Inode, RamNode>,
    next_inode: Inode,
}

impl RamFs {
    pub fn new() -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(1, RamNode::directory());
        Self {
            nodes,
            next_inode: 2,
        }
    }

    fn node(&self, inode: Inode) -> Result<&RamNode, VfsError> {
        self.nodes.get(&inode).ok_or(VfsError::NotFound)
    }

    fn node_mut(&mut self, inode: Inode) -> Result<&mut RamNode, VfsError> {
        self.nodes.get_mut(&inode).ok_or(VfsError::NotFound)
    }

    fn create_node(
        &mut self,
        parent: Inode,
        name: &str,
        kind: NodeKind,
    ) -> Result<Inode, VfsError> {
        validate_name(name)?;
        {
            let directory = self.node(parent)?;
            if directory.kind != NodeKind::Directory {
                return Err(VfsError::NotDirectory);
            }
            if directory.children.contains_key(name) {
                return Err(VfsError::AlreadyExists);
            }
        }

        let inode = self.next_inode;
        self.next_inode = self
            .next_inode
            .checked_add(1)
            .ok_or(VfsError::InodeExhausted)?;

        let node = match kind {
            NodeKind::File => RamNode::file(),
            NodeKind::Directory => RamNode::directory(),
        };
        self.nodes.insert(inode, node);
        self.node_mut(parent)?
            .children
            .insert(name.to_string(), inode);
        Ok(inode)
    }
}

impl Default for RamFs {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem for RamFs {
    fn name(&self) -> &'static str {
        "ramfs"
    }

    fn root_inode(&self) -> Inode {
        1
    }

    fn metadata(&self, inode: Inode) -> Result<Metadata, VfsError> {
        let node = self.node(inode)?;
        Ok(Metadata {
            inode,
            kind: node.kind,
            len: match node.kind {
                NodeKind::File => node.data.len() as u64,
                NodeKind::Directory => 0,
            },
        })
    }

    fn lookup(&self, parent: Inode, name: &str) -> Result<Inode, VfsError> {
        validate_name(name)?;
        let directory = self.node(parent)?;
        if directory.kind != NodeKind::Directory {
            return Err(VfsError::NotDirectory);
        }
        directory
            .children
            .get(name)
            .copied()
            .ok_or(VfsError::NotFound)
    }

    fn read_dir(&self, inode: Inode) -> Result<Vec<DirEntry>, VfsError> {
        let directory = self.node(inode)?;
        if directory.kind != NodeKind::Directory {
            return Err(VfsError::NotDirectory);
        }

        let mut entries = Vec::with_capacity(directory.children.len());
        for (name, child) in &directory.children {
            entries.push(DirEntry {
                name: name.clone(),
                metadata: self.metadata(*child)?,
            });
        }
        Ok(entries)
    }

    fn create_file(&mut self, parent: Inode, name: &str) -> Result<Inode, VfsError> {
        self.create_node(parent, name, NodeKind::File)
    }

    fn create_dir(&mut self, parent: Inode, name: &str) -> Result<Inode, VfsError> {
        self.create_node(parent, name, NodeKind::Directory)
    }

    fn read(&self, inode: Inode, offset: usize, buffer: &mut [u8]) -> Result<usize, VfsError> {
        let node = self.node(inode)?;
        if node.kind == NodeKind::Directory {
            return Err(VfsError::IsDirectory);
        }
        if offset >= node.data.len() {
            return Ok(0);
        }

        let count = buffer.len().min(node.data.len() - offset);
        buffer[..count].copy_from_slice(&node.data[offset..offset + count]);
        Ok(count)
    }

    fn write(&mut self, inode: Inode, offset: usize, data: &[u8]) -> Result<usize, VfsError> {
        let node = self.node_mut(inode)?;
        if node.kind == NodeKind::Directory {
            return Err(VfsError::IsDirectory);
        }

        let end = offset
            .checked_add(data.len())
            .ok_or(VfsError::OffsetOverflow)?;
        if end > node.data.len() {
            node.data.resize(end, 0);
        }
        node.data[offset..end].copy_from_slice(data);
        Ok(data.len())
    }

    fn truncate(&mut self, inode: Inode, len: usize) -> Result<(), VfsError> {
        let node = self.node_mut(inode)?;
        if node.kind == NodeKind::Directory {
            return Err(VfsError::IsDirectory);
        }
        node.data.resize(len, 0);
        Ok(())
    }

    fn remove(&mut self, parent: Inode, name: &str) -> Result<(), VfsError> {
        validate_name(name)?;

        let child = {
            let directory = self.node(parent)?;
            if directory.kind != NodeKind::Directory {
                return Err(VfsError::NotDirectory);
            }
            directory
                .children
                .get(name)
                .copied()
                .ok_or(VfsError::NotFound)?
        };

        let child_node = self.node(child)?;
        if child_node.kind == NodeKind::Directory && !child_node.children.is_empty() {
            return Err(VfsError::DirectoryNotEmpty);
        }

        self.node_mut(parent)?.children.remove(name);
        self.nodes.remove(&child);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vfs() -> Vfs {
        Vfs::new(Box::new(RamFs::new()))
    }

    #[test]
    fn normalizes_absolute_and_relative_paths() {
        assert_eq!(normalize_path("/", "/a//b/./c").unwrap(), "/a/b/c");
        assert_eq!(
            normalize_path("/home/user", "../tmp/../notes").unwrap(),
            "/home/notes"
        );
        assert_eq!(normalize_path("/", "../../../../etc").unwrap(), "/etc");
        assert_eq!(normalize_path("/a", ".").unwrap(), "/a");
    }

    #[test]
    fn creates_reads_writes_and_truncates_files() {
        let mut vfs = vfs();
        vfs.create_dir("/etc").unwrap();
        let metadata = vfs.create_file("/etc/motd").unwrap();
        assert_eq!(metadata.kind, NodeKind::File);

        assert_eq!(vfs.write("/etc/motd", 0, b"Generic").unwrap(), 7);
        assert_eq!(vfs.write("/etc/motd", 7, b" OS").unwrap(), 3);
        assert_eq!(vfs.read_all("/etc/motd").unwrap(), b"Generic OS");

        vfs.truncate("/etc/motd", 7).unwrap();
        assert_eq!(vfs.read_all("/etc/motd").unwrap(), b"Generic");
        assert_eq!(vfs.metadata("/etc/motd").unwrap().len, 7);
    }

    #[test]
    fn sparse_write_zero_fills_gap() {
        let mut vfs = vfs();
        vfs.create_file("/data").unwrap();
        vfs.write("/data", 3, b"x").unwrap();
        assert_eq!(vfs.read_all("/data").unwrap(), &[0, 0, 0, b'x']);
    }

    #[test]
    fn directory_listing_is_sorted_and_remove_is_safe() {
        let mut vfs = vfs();
        vfs.create_dir("/tmp").unwrap();
        vfs.create_file("/tmp/z").unwrap();
        vfs.create_file("/tmp/a").unwrap();

        let entries = vfs.read_dir("/tmp").unwrap();
        assert_eq!(entries[0].name, "a");
        assert_eq!(entries[1].name, "z");
        assert_eq!(vfs.remove("/tmp"), Err(VfsError::DirectoryNotEmpty));

        vfs.remove("/tmp/a").unwrap();
        vfs.remove("/tmp/z").unwrap();
        vfs.remove("/tmp").unwrap();
        assert_eq!(vfs.metadata("/tmp"), Err(VfsError::NotFound));
    }

    #[test]
    fn mount_routes_to_deepest_filesystem() {
        let mut vfs = vfs();
        vfs.create_dir("/mnt").unwrap();
        vfs.mount("/mnt", Box::new(RamFs::new())).unwrap();

        vfs.create_file("/root-file").unwrap();
        vfs.create_file("/mnt/mounted-file").unwrap();
        assert_eq!(vfs.write("/mnt/mounted-file", 0, b"mounted").unwrap(), 7);
        assert_eq!(vfs.read_all("/mnt/mounted-file").unwrap(), b"mounted");
        assert_eq!(vfs.mounts().len(), 2);
        assert_eq!(
            vfs.mount("/mnt", Box::new(RamFs::new())),
            Err(VfsError::AlreadyMounted)
        );
    }

    #[test]
    fn rejects_invalid_operations() {
        let mut vfs = vfs();
        vfs.create_file("/file").unwrap();
        assert_eq!(vfs.create_file("/file/child"), Err(VfsError::NotDirectory));
        assert_eq!(vfs.read_dir("/file"), Err(VfsError::NotDirectory));
        assert_eq!(vfs.read_all("/"), Err(VfsError::IsDirectory));
        assert_eq!(vfs.remove("/"), Err(VfsError::RootImmutable));
        assert_eq!(vfs.create_file("/file"), Err(VfsError::AlreadyExists));
    }
}
