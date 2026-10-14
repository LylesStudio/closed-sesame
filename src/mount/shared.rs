//! Backend-agnostic shared vault state for mount-style frontends.
//!
//! Both the FUSE adapter and the WebDAV bridge serve reads from a VFS
//! snapshot and serialize every container mutation through a single writer
//! mutex, refreshing the snapshot after each committed change. All
//! replacement writes go through the container writer's two-phase commit
//! (new pages durable before old pages are released), so an interrupted
//! write can never destroy the previous version of a file.

use super::vfs::{DirectoryEntry, Metadata, ReadOnlyVfs};
use crate::container::{ContainerReader, ContainerWriter};
use crate::error::{Result, SesameError};
use crate::keymgmt::KeyEncryptingKey;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard};

pub(crate) struct SharedVault {
    vfs: RwLock<ReadOnlyVfs<ContainerReader>>,
    writer: Mutex<ContainerWriter>,
    container: PathBuf,
    kek: KeyEncryptingKey,
}

impl SharedVault {
    pub(crate) fn open(container: &Path, kek: &KeyEncryptingKey) -> Result<Self> {
        Ok(Self {
            vfs: RwLock::new(ReadOnlyVfs::open(container, kek)?),
            writer: Mutex::new(ContainerWriter::open(container, kek)?),
            container: container.to_path_buf(),
            kek: kek.clone(),
        })
    }

    pub(crate) fn vfs(&self) -> RwLockReadGuard<'_, ReadOnlyVfs<ContainerReader>> {
        self.vfs
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Serialize a container mutation across every in-process frontend
    /// (FUSE commits, WebDAV requests, GUI actions, cache write-back).
    pub(crate) fn mutation_lock(&self) -> MutexGuard<'_, ContainerWriter> {
        self.writer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Rebuild the read view from the on-disk container.
    pub(crate) fn reload_vfs(&self) -> Result<()> {
        let fresh = ReadOnlyVfs::open(&self.container, &self.kek)?;
        let mut vfs = self
            .vfs
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *vfs = fresh;
        Ok(())
    }

    /// Run a mutation under the writer mutex and refresh the read view.
    pub(crate) fn mutate(&self, f: impl FnOnce(&mut ContainerWriter) -> Result<()>) -> Result<()> {
        {
            let mut writer = self.mutation_lock();
            f(&mut writer)?;
        }
        self.reload_vfs()
    }

    /// Metadata for a container path (`""` or `/` is the root directory).
    pub(crate) fn metadata_of(&self, path: &str) -> Option<Metadata> {
        let vfs = self.vfs();
        if path.is_empty() {
            return vfs.metadata(super::vfs::ROOT_INODE);
        }
        let inode = vfs.lookup_path(path)?;
        vfs.metadata(inode)
    }

    /// Immediate children of a directory path (without `.`/`..`).
    pub(crate) fn list_dir(&self, path: &str) -> Option<Vec<DirectoryEntry>> {
        let vfs = self.vfs();
        let inode = if path.is_empty() {
            super::vfs::ROOT_INODE
        } else {
            vfs.lookup_path(path)?
        };
        let entries = vfs.readdir(inode, 0)?;
        Some(
            entries
                .into_iter()
                .filter(|entry| entry.name != "." && entry.name != "..")
                .collect(),
        )
    }

    /// Read a whole file from the current snapshot.
    pub(crate) fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let vfs = self.vfs();
        let inode = vfs
            .lookup_path(path)
            .ok_or_else(|| SesameError::InvalidInput(format!("path not found: {path}")))?;
        vfs.read(inode, 0, u64::MAX as usize)
    }

    /// Create or replace a file with a two-phase commit, then refresh.
    pub(crate) fn write_file(&self, path: &str, data: &[u8]) -> Result<()> {
        self.mutate(|writer| {
            if writer.find_entry(path).is_some() {
                writer.replace_file_data(path, data).map(|_| ())
            } else {
                writer.add_file_data(data, path, None, false).map(|_| ())
            }
        })
    }

    pub(crate) fn mkdir(&self, path: &str) -> Result<()> {
        self.mutate(|writer| writer.mkdir(path).map(|_| ()))
    }

    pub(crate) fn remove(&self, path: &str) -> Result<()> {
        self.mutate(|writer| writer.remove_path(path).map(|_| ()))
    }

    pub(crate) fn rename(&self, old: &str, new: &str, overwrite: bool) -> Result<()> {
        self.mutate(|writer| {
            if overwrite && let Some(dest) = writer.find_entry(new) {
                if dest.is_directory {
                    return Err(SesameError::InvalidInput(format!(
                        "destination already exists: {new}"
                    )));
                }
                writer.remove_path(new)?;
            }
            writer.rename_path(old, new)
        })
    }
}

/// Cloneable handle that lets a background thread take the mount writer lock.
#[derive(Clone)]
pub struct MutationTicket {
    shared: Arc<SharedVault>,
}

impl MutationTicket {
    pub(crate) fn new(shared: Arc<SharedVault>) -> Self {
        Self { shared }
    }

    pub fn lock(&self) -> MutexGuard<'_, ContainerWriter> {
        self.shared.mutation_lock()
    }
}
