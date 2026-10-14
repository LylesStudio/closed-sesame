//! Shared state behind a FUSE mount: the read view, the serialized writer,
//! and the table of open write handles.
//!
//! Robustness rules (in priority order: never corrupt, never lose data):
//! - All container mutations go through the single writer mutex (shared with
//!   WebDAV/GUI/cache mutations via [`SharedVault::mutation_lock`]).
//! - File writes are copy-on-write-on-close: edits accumulate in a memory
//!   buffer and are committed on flush/release through the writer's
//!   two-phase commit (new pages durable before old pages are released).
//! - After every committed mutation the VFS is reloaded from disk so the
//!   mounted view never points at stale pages.

use super::shared::SharedVault;
use super::vfs::ReadOnlyVfs;
use crate::container::{ContainerReader, ContainerWriter};
use crate::error::Result;
use crate::keymgmt::KeyEncryptingKey;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// Buffer for one open-for-writing file handle.
#[derive(Debug)]
pub(crate) struct OpenHandle {
    pub(crate) path: String,
    pub(crate) buffer: Vec<u8>,
    pub(crate) dirty: bool,
}

pub(crate) struct FuseMountState {
    shared: Arc<SharedVault>,
    handles: Mutex<HashMap<u64, OpenHandle>>,
    next_fh: AtomicU64,
    read_only: bool,
}

impl FuseMountState {
    pub(crate) fn open(container: &Path, kek: &KeyEncryptingKey, read_only: bool) -> Result<Self> {
        Ok(Self {
            shared: Arc::new(SharedVault::open(container, kek)?),
            handles: Mutex::new(HashMap::new()),
            next_fh: AtomicU64::new(1),
            read_only,
        })
    }

    pub(crate) fn read_only(&self) -> bool {
        self.read_only
    }

    pub(crate) fn vfs(&self) -> std::sync::RwLockReadGuard<'_, ReadOnlyVfs<ContainerReader>> {
        self.shared.vfs()
    }

    /// Serialize a container mutation with FUSE-side commits. Callers outside
    /// FUSE (GUI actions, cache write-back) hold this guard across their own
    /// writer operations so a FUSE flush cannot interleave mid-mutation.
    pub(crate) fn mutation_lock(&self) -> MutexGuard<'_, ContainerWriter> {
        self.shared.mutation_lock()
    }

    pub(crate) fn ticket(&self) -> super::shared::MutationTicket {
        super::shared::MutationTicket::new(Arc::clone(&self.shared))
    }

    /// Rebuild the read view from the on-disk container.
    pub(crate) fn reload_vfs(&self) -> Result<()> {
        self.shared.reload_vfs()
    }

    pub(crate) fn alloc_handle(&self, handle: OpenHandle) -> u64 {
        let fh = self.next_fh.fetch_add(1, Ordering::Relaxed);
        self.handles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(fh, handle);
        fh
    }

    pub(crate) fn with_handle<R>(
        &self,
        fh: u64,
        f: impl FnOnce(&mut OpenHandle) -> R,
    ) -> Option<R> {
        let mut handles = self
            .handles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        handles.get_mut(&fh).map(f)
    }

    pub(crate) fn take_handle(&self, fh: u64) -> Option<OpenHandle> {
        self.handles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&fh)
    }

    /// Truncate/extend every open write handle for `path`. The kernel
    /// delivers O_TRUNC as `setattr(size=0)` — sometimes without a file
    /// handle — so truncation must reach handles by path, not only by fh.
    /// Returns true when at least one open handle was resized.
    pub(crate) fn resize_handles_for_path(&self, path: &str, new_size: u64) -> bool {
        let mut handles = self
            .handles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut found = false;
        for handle in handles.values_mut() {
            if handle.path == path {
                handle.buffer.resize(new_size as usize, 0);
                handle.dirty = true;
                found = true;
            }
        }
        found
    }

    /// Commit a dirty handle's buffer into the container, then refresh the
    /// read view. Safe to call repeatedly (flush may fire multiple times).
    pub(crate) fn commit_handle(&self, fh: u64) -> Result<()> {
        let pending = self.with_handle(fh, |handle| {
            if handle.dirty {
                Some((handle.path.clone(), handle.buffer.clone()))
            } else {
                None
            }
        });
        let Some((path, buffer)) = pending.flatten() else {
            return Ok(());
        };
        self.shared.write_file(&path, &buffer)?;
        self.with_handle(fh, |handle| handle.dirty = false);
        Ok(())
    }

    /// Run a metadata mutation (mkdir/unlink/rmdir/rename/truncate) under the
    /// writer mutex and refresh the read view afterwards.
    pub(crate) fn mutate(&self, f: impl FnOnce(&mut ContainerWriter) -> Result<()>) -> Result<()> {
        self.shared.mutate(f)
    }
}
