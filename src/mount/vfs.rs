//! Platform-independent metadata and page view of a vault.

use crate::container::format::PAGE_PLAINTEXT;
use crate::container::path::{ancestors, file_name, normalize, parent};
use crate::container::writer::validate_entry_tree;
use crate::container::{ContainerReader, FileEntry};
use crate::error::{Result, SesameError};
use crate::i18n::keys;
use crate::keymgmt::KeyEncryptingKey;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub(crate) const ROOT_INODE: u64 = 1;
const FIRST_INODE: u64 = 2;

pub(crate) trait PageReader: Send + Sync {
    fn list_files(&self) -> Result<Vec<FileEntry>>;
    fn read_page(&self, page: u64) -> Result<Vec<u8>>;
}

impl PageReader for ContainerReader {
    fn list_files(&self) -> Result<Vec<FileEntry>> {
        ContainerReader::list_files(self)
    }

    fn read_page(&self, page: u64) -> Result<Vec<u8>> {
        ContainerReader::read_page(self, page)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Metadata {
    pub inode: u64,
    pub kind: NodeKind,
    pub size: u64,
    pub created_at: u64,
    pub modified_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirectoryEntry {
    pub inode: u64,
    pub name: String,
    pub kind: NodeKind,
}

struct NodeState {
    path: String,
    parent_inode: u64,
    entry_index: Option<usize>,
    kind: NodeKind,
    size: u64,
    created_at: u64,
    modified_at: u64,
}

pub(crate) struct ReadOnlyVfs<R = ContainerReader> {
    reader: R,
    entries: Vec<FileEntry>,
    nodes: HashMap<u64, NodeState>,
    children: HashMap<u64, BTreeMap<String, u64>>,
}

impl ReadOnlyVfs<ContainerReader> {
    pub(crate) fn open(container: &Path, kek: &KeyEncryptingKey) -> Result<Self> {
        Self::from_reader(ContainerReader::open(container, kek)?)
    }
}

impl<R: PageReader> ReadOnlyVfs<R> {
    fn from_reader(reader: R) -> Result<Self> {
        let entries = reader.list_files()?;
        validate_entry_tree(&entries)?;

        let mut path_to_entry = BTreeMap::new();
        for (index, entry) in entries.iter().enumerate() {
            let path = normalize(&entry.path)?;
            if path_to_entry.insert(path, index).is_some() {
                return Err(SesameError::Corruption(format!(
                    "duplicate container path: {}",
                    entry.path
                )));
            }
        }

        let mut all_paths = BTreeMap::new();
        for path in path_to_entry.keys() {
            all_paths.insert(path.clone(), ());
            for ancestor in ancestors(path) {
                all_paths.insert(ancestor, ());
            }
        }

        let mut path_to_inode = BTreeMap::new();
        let mut inode = FIRST_INODE;
        for path in all_paths.keys() {
            path_to_inode.insert(path.clone(), inode);
            inode += 1;
        }

        let mut nodes = HashMap::new();
        for (path, &node_inode) in &path_to_inode {
            let entry_index = path_to_entry.get(path).copied();
            let parent_inode = match parent(path) {
                Some(parent_path) => *path_to_inode
                    .get(parent_path)
                    .ok_or_else(|| SesameError::Corruption(format!("missing parent for {path}")))?,
                None => ROOT_INODE,
            };
            let (kind, size, created_at, modified_at) = if let Some(index) = entry_index {
                let entry = &entries[index];
                (
                    if entry.is_directory {
                        NodeKind::Directory
                    } else {
                        NodeKind::File
                    },
                    entry.size,
                    entry.created_at,
                    entry.modified_at,
                )
            } else {
                (NodeKind::Directory, 0, 0, 0)
            };
            nodes.insert(
                node_inode,
                NodeState {
                    path: path.clone(),
                    parent_inode,
                    entry_index,
                    kind,
                    size,
                    created_at,
                    modified_at,
                },
            );
        }

        let mut children = HashMap::new();
        children.insert(ROOT_INODE, BTreeMap::new());
        for (path, &node_inode) in &path_to_inode {
            let parent_inode = match parent(path) {
                Some(parent_path) => *path_to_inode
                    .get(parent_path)
                    .ok_or_else(|| SesameError::Corruption(format!("missing parent for {path}")))?,
                None => ROOT_INODE,
            };
            let name = file_name(path).to_owned();
            children
                .entry(parent_inode)
                .or_default()
                .insert(name, node_inode);
        }

        Ok(Self {
            reader,
            entries,
            nodes,
            children,
        })
    }

    pub(crate) fn lookup(&self, parent: u64, name: &str) -> Option<u64> {
        match name {
            "." => Some(parent),
            ".." => Some(
                self.nodes
                    .get(&parent)
                    .map(|node| node.parent_inode)
                    .unwrap_or(ROOT_INODE),
            ),
            _ => self.children.get(&parent)?.get(name).copied(),
        }
    }

    /// Container path of a node (`None` for the root or unknown inodes).
    pub(crate) fn path(&self, inode: u64) -> Option<String> {
        self.nodes.get(&inode).map(|node| node.path.clone())
    }

    /// Container path of a directory inode plus a child name.
    pub(crate) fn child_path(&self, parent: u64, name: &str) -> Option<String> {
        if name.is_empty() || name.contains('/') || name == "." || name == ".." {
            return None;
        }
        let metadata = self.metadata(parent)?;
        if metadata.kind != NodeKind::Directory {
            return None;
        }
        Some(match self.path(parent) {
            Some(parent_path) => format!("{parent_path}/{name}"),
            None => name.to_owned(),
        })
    }

    /// Resolve a full container path (e.g. `docs/a.txt`) to an inode.
    pub(crate) fn lookup_path(&self, path: &str) -> Option<u64> {
        let mut inode = ROOT_INODE;
        for segment in path.split('/').filter(|segment| !segment.is_empty()) {
            inode = self.lookup(inode, segment)?;
        }
        Some(inode)
    }

    pub(crate) fn metadata(&self, inode: u64) -> Option<Metadata> {
        if inode == ROOT_INODE {
            return Some(Metadata {
                inode,
                kind: NodeKind::Directory,
                size: 0,
                created_at: 0,
                modified_at: 0,
            });
        }
        let node = self.nodes.get(&inode)?;
        Some(Metadata {
            inode,
            kind: node.kind,
            size: node.size,
            created_at: node.created_at,
            modified_at: node.modified_at,
        })
    }

    pub(crate) fn readdir(&self, inode: u64, offset: usize) -> Option<Vec<DirectoryEntry>> {
        if inode != ROOT_INODE && !self.nodes.contains_key(&inode) {
            return None;
        }
        if self.metadata(inode)?.kind == NodeKind::File {
            return None;
        }

        let mut result = vec![
            DirectoryEntry {
                inode,
                name: ".".into(),
                kind: NodeKind::Directory,
            },
            DirectoryEntry {
                inode: self
                    .nodes
                    .get(&inode)
                    .map(|node| node.parent_inode)
                    .unwrap_or(ROOT_INODE),
                name: "..".into(),
                kind: NodeKind::Directory,
            },
        ];

        if let Some(child_map) = self.children.get(&inode) {
            result.extend(child_map.iter().map(|(name, child_inode)| {
                let kind = self
                    .metadata(*child_inode)
                    .map(|meta| meta.kind)
                    .unwrap_or(NodeKind::Directory);
                DirectoryEntry {
                    inode: *child_inode,
                    name: name.clone(),
                    kind,
                }
            }));
        }

        Some(result.into_iter().skip(offset).collect())
    }

    pub(crate) fn read(&self, inode: u64, offset: u64, size: usize) -> Result<Vec<u8>> {
        if inode == ROOT_INODE {
            return Err(SesameError::InvalidInput(
                keys::CANNOT_READ_ROOT_AS_FILE.into(),
            ));
        }
        let node = self
            .nodes
            .get(&inode)
            .ok_or_else(|| SesameError::InvalidInput(format!("unknown inode {inode}")))?;
        if node.kind == NodeKind::Directory {
            return Err(SesameError::InvalidInput(format!(
                "cannot read directory as a file: {}",
                node.path
            )));
        }
        let entry = node
            .entry_index
            .and_then(|index| self.entries.get(index))
            .ok_or_else(|| SesameError::Corruption(format!("missing file entry for {inode}")))?;

        if size == 0 || offset >= entry.size {
            return Ok(Vec::new());
        }

        let end = offset.saturating_add(size as u64).min(entry.size);
        let page_size = PAGE_PLAINTEXT as u64;
        let first_page = offset / page_size;
        let last_page = end.div_ceil(page_size);
        let mut output = Vec::with_capacity((end - offset) as usize);

        for file_page in first_page..last_page {
            if file_page >= entry.page_count {
                return Err(SesameError::Corruption(format!(
                    "file {} metadata does not cover requested page",
                    entry.path
                )));
            }
            let page = self.reader.read_page(entry.start_page + file_page)?;
            let page_start = file_page * page_size;
            let copy_start = offset.saturating_sub(page_start) as usize;
            let copy_end = (end - page_start).min(page_size) as usize;
            if copy_start > copy_end || copy_end > page.len() {
                return Err(SesameError::Corruption(format!(
                    "decrypted page for {} is shorter than its metadata",
                    entry.path
                )));
            }
            output.extend_from_slice(&page[copy_start..copy_end]);
        }
        Ok(output)
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn total_file_size(&self) -> u64 {
        self.entries
            .iter()
            .filter(|entry| !entry.is_directory)
            .map(|entry| entry.size)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockReader {
        entries: Vec<FileEntry>,
        pages: HashMap<u64, Vec<u8>>,
    }

    impl PageReader for MockReader {
        fn list_files(&self) -> Result<Vec<FileEntry>> {
            Ok(self.entries.clone())
        }

        fn read_page(&self, page: u64) -> Result<Vec<u8>> {
            self.pages
                .get(&page)
                .cloned()
                .ok_or_else(|| SesameError::Corruption(format!("missing test page {page}")))
        }
    }

    fn file(path: &str, size: u64, start_page: u64, page_count: u64) -> FileEntry {
        FileEntry {
            path: path.into(),
            original_path: None,
            size,
            start_page,
            page_count,
            crc32: 0,
            created_at: 10,
            modified_at: 20,
            has_symlink: false,
            is_directory: false,
        }
    }

    fn directory(path: &str) -> FileEntry {
        FileEntry {
            path: path.into(),
            original_path: None,
            size: 0,
            start_page: 0,
            page_count: 0,
            crc32: 0,
            created_at: 5,
            modified_at: 5,
            has_symlink: false,
            is_directory: true,
        }
    }

    fn vfs(entries: Vec<FileEntry>, pages: HashMap<u64, Vec<u8>>) -> ReadOnlyVfs<MockReader> {
        ReadOnlyVfs::from_reader(MockReader { entries, pages }).unwrap()
    }

    #[test]
    fn nested_lookup_and_readdir() {
        let vfs = vfs(
            vec![
                directory("docs"),
                file("docs/readme.txt", 4, 1, 1),
                file("root.txt", 2, 2, 1),
            ],
            HashMap::new(),
        );
        let docs = vfs.lookup(ROOT_INODE, "docs").unwrap();
        assert_eq!(vfs.metadata(docs).unwrap().kind, NodeKind::Directory);
        let readme = vfs.lookup(docs, "readme.txt").unwrap();
        assert_eq!(vfs.metadata(readme).unwrap().kind, NodeKind::File);

        let names: Vec<_> = vfs
            .readdir(ROOT_INODE, 0)
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        assert!(names.contains(&"docs".to_string()));
        assert!(names.contains(&"root.txt".to_string()));
    }

    #[test]
    fn reads_ranges_within_and_across_pages_and_clamps_at_eof() {
        let mut pages = HashMap::new();
        pages.insert(7, vec![b'a'; PAGE_PLAINTEXT]);
        pages.insert(8, vec![b'b'; PAGE_PLAINTEXT]);
        let size = PAGE_PLAINTEXT as u64 + 3;
        let vfs = vfs(vec![file("data.bin", size, 7, 2)], pages);
        let inode = vfs.lookup(ROOT_INODE, "data.bin").unwrap();

        assert_eq!(vfs.read(inode, 5, 3).unwrap(), b"aaa");
        assert_eq!(
            vfs.read(inode, PAGE_PLAINTEXT as u64 - 2, 5).unwrap(),
            b"aabbb"
        );
    }

    #[test]
    fn rejects_orphan_and_conflicting_paths() {
        assert!(
            ReadOnlyVfs::from_reader(MockReader {
                entries: vec![file("orphan/child.txt", 1, 0, 1)],
                pages: HashMap::new(),
            })
            .is_err()
        );

        let file_and_dir = file("conflict", 1, 0, 1);
        let mut dir = directory("conflict/child");
        dir.path = "conflict/nested".into();
        assert!(
            ReadOnlyVfs::from_reader(MockReader {
                entries: vec![file_and_dir, dir],
                pages: HashMap::new(),
            })
            .is_err()
        );
    }
}
