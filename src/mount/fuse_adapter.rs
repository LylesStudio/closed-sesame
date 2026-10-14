//! Shared Linux/macOS adapter from the vault to `fuser`.
//!
//! Read operations are served from the VFS snapshot; write operations are
//! copy-on-write-on-close through [`FuseMountState`]: edits accumulate in a
//! per-handle buffer and commit on flush/release via the container writer's
//! two-phase commit, so an interrupted write can never destroy the previous
//! version of a file.

use super::fuse_state::{FuseMountState, OpenHandle};
use super::vfs::{Metadata, NodeKind, ReadOnlyVfs};
use crate::container::ContainerReader;
use crate::error::{Result, SesameError};
use crate::i18n::keys;
use fuser::{
    BackgroundSession, Config, Errno, FileAttr, FileHandle, FileType, Filesystem, FopenFlags,
    Generation, INodeNo, LockOwner, MountOption, OpenAccMode, OpenFlags, ReplyAttr, ReplyCreate,
    ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyWrite, Request, TimeOrNow,
};
use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

const TTL: Duration = Duration::from_secs(1);

struct FuseFilesystem {
    state: Arc<FuseMountState>,
    uid: u32,
    gid: u32,
}

fn errno_for(error: &SesameError) -> Errno {
    match error {
        SesameError::DiskFull { .. } => Errno::ENOSPC,
        SesameError::InvalidInput(message) => {
            if message.contains("not found") {
                Errno::ENOENT
            } else if message.contains("already exists") {
                Errno::EEXIST
            } else {
                Errno::EIO
            }
        }
        _ => Errno::EIO,
    }
}

impl FuseFilesystem {
    fn new(state: Arc<FuseMountState>) -> Self {
        Self {
            state,
            // SAFETY: getuid/getgid have no preconditions and do not mutate memory.
            uid: unsafe { libc::getuid() },
            gid: unsafe { libc::getgid() },
        }
    }

    fn vfs(&self) -> std::sync::RwLockReadGuard<'_, ReadOnlyVfs<ContainerReader>> {
        self.state.vfs()
    }

    fn attr(&self, metadata: &Metadata) -> FileAttr {
        let kind = match metadata.kind {
            NodeKind::Directory => FileType::Directory,
            NodeKind::File => FileType::RegularFile,
        };
        let created = unix_time(metadata.created_at);
        let modified = unix_time(metadata.modified_at);
        let read_only = self.state.read_only();
        FileAttr {
            ino: INodeNo(metadata.inode),
            size: metadata.size,
            blocks: metadata.size.div_ceil(512),
            atime: modified,
            mtime: modified,
            ctime: created,
            crtime: created,
            kind,
            perm: match (metadata.kind, read_only) {
                (NodeKind::Directory, true) => 0o555,
                (NodeKind::Directory, false) => 0o755,
                (NodeKind::File, true) => 0o444,
                (NodeKind::File, false) => 0o644,
            },
            nlink: if metadata.kind == NodeKind::Directory {
                2
            } else {
                1
            },
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            flags: 0,
            blksize: 4096,
        }
    }

    /// Current attr for a container path, when it still exists after a
    /// mutation-triggered VFS reload.
    fn attr_for_path(&self, path: &str) -> Option<FileAttr> {
        let vfs = self.vfs();
        let inode = vfs.lookup_path(path)?;
        vfs.metadata(inode).map(|metadata| self.attr(&metadata))
    }
}

impl Filesystem for FuseFilesystem {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let Some(name) = name.to_str() else {
            reply.error(Errno::ENOENT);
            return;
        };
        let vfs = self.vfs();
        let Some(inode) = vfs.lookup(parent.into(), name) else {
            reply.error(Errno::ENOENT);
            return;
        };
        let Some(metadata) = vfs.metadata(inode) else {
            reply.error(Errno::ENOENT);
            return;
        };
        reply.entry(&TTL, &self.attr(&metadata), Generation(0));
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, fh: Option<FileHandle>, reply: ReplyAttr) {
        // Serve the size of a dirty write buffer so editors see their own
        // uncommitted edits.
        if let Some(FileHandle(handle)) = fh
            && handle != 0
            && let Some(len) = self
                .state
                .with_handle(handle, |pending| pending.buffer.len())
            && let Some(mut attr) = self
                .vfs()
                .metadata(ino.into())
                .map(|metadata| self.attr(&metadata))
        {
            attr.size = len as u64;
            attr.blocks = (len as u64).div_ceil(512);
            reply.attr(&TTL, &attr);
            return;
        }
        match self.vfs().metadata(ino.into()) {
            Some(metadata) => reply.attr(&TTL, &self.attr(&metadata)),
            None => reply.error(Errno::ENOENT),
        }
    }

    fn setattr(
        &self,
        _req: &Request,
        ino: INodeNo,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<TimeOrNow>,
        _mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        fh: Option<FileHandle>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<fuser::BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        if let Some(new_size) = size {
            if self.state.read_only() {
                reply.error(Errno::EROFS);
                return;
            }
            let Some(path) = self.vfs().path(ino.into()) else {
                reply.error(Errno::ENOENT);
                return;
            };
            let _ = fh;
            // The kernel delivers O_TRUNC as setattr(size) — often without a
            // file handle — so truncate open write buffers by path. When a
            // handle is open, the commit happens on flush/release.
            if self.state.resize_handles_for_path(&path, new_size) {
                match self.attr_for_path(&path) {
                    Some(mut attr) => {
                        attr.size = new_size;
                        attr.blocks = new_size.div_ceil(512);
                        reply.attr(&TTL, &attr);
                    }
                    None => reply.error(Errno::ENOENT),
                }
                return;
            }
            // No open handle: read-modify-write with an immediate commit.
            let current = self
                .vfs()
                .read(ino.into(), 0, u32::MAX as usize)
                .unwrap_or_default();
            let mut resized = current;
            resized.resize(new_size as usize, 0);
            let target = path.clone();
            if let Err(error) = self
                .state
                .mutate(|writer| writer.replace_file_data(&target, &resized).map(|_| ()))
            {
                reply.error(errno_for(&error));
                return;
            }
            match self.attr_for_path(&path) {
                Some(attr) => reply.attr(&TTL, &attr),
                None => reply.error(Errno::ENOENT),
            }
            return;
        }
        // Permission/timestamp changes are accepted and ignored: the vault
        // owns its own metadata model.
        match self.vfs().metadata(ino.into()) {
            Some(metadata) => reply.attr(&TTL, &self.attr(&metadata)),
            None => reply.error(Errno::ENOENT),
        }
    }

    fn mkdir(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        if self.state.read_only() {
            reply.error(Errno::EROFS);
            return;
        }
        let Some(name) = name.to_str() else {
            reply.error(Errno::EINVAL);
            return;
        };
        let Some(path) = self.vfs().child_path(parent.into(), name) else {
            reply.error(Errno::ENOENT);
            return;
        };
        if self.vfs().lookup(parent.into(), name).is_some() {
            reply.error(Errno::EEXIST);
            return;
        }
        if let Err(error) = self.state.mutate(|writer| writer.mkdir(&path).map(|_| ())) {
            reply.error(errno_for(&error));
            return;
        }
        match self.attr_for_path(&path) {
            Some(attr) => reply.entry(&TTL, &attr, Generation(0)),
            None => reply.error(Errno::EIO),
        }
    }

    fn unlink(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        if self.state.read_only() {
            reply.error(Errno::EROFS);
            return;
        }
        let Some(name) = name.to_str() else {
            reply.error(Errno::EINVAL);
            return;
        };
        let Some(inode) = self.vfs().lookup(parent.into(), name) else {
            reply.error(Errno::ENOENT);
            return;
        };
        if self
            .vfs()
            .metadata(inode)
            .is_some_and(|meta| meta.kind == NodeKind::Directory)
        {
            reply.error(Errno::EISDIR);
            return;
        }
        let Some(path) = self.vfs().path(inode) else {
            reply.error(Errno::ENOENT);
            return;
        };
        match self
            .state
            .mutate(|writer| writer.remove_path(&path).map(|_| ()))
        {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(errno_for(&error)),
        }
    }

    fn rmdir(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        if self.state.read_only() {
            reply.error(Errno::EROFS);
            return;
        }
        let Some(name) = name.to_str() else {
            reply.error(Errno::EINVAL);
            return;
        };
        let Some(inode) = self.vfs().lookup(parent.into(), name) else {
            reply.error(Errno::ENOENT);
            return;
        };
        if !self
            .vfs()
            .metadata(inode)
            .is_some_and(|meta| meta.kind == NodeKind::Directory)
        {
            reply.error(Errno::ENOTDIR);
            return;
        }
        let has_children = self
            .vfs()
            .readdir(inode, 0)
            .map(|entries| {
                entries
                    .iter()
                    .any(|entry| entry.name != "." && entry.name != "..")
            })
            .unwrap_or(false);
        if has_children {
            reply.error(Errno::ENOTEMPTY);
            return;
        }
        let Some(path) = self.vfs().path(inode) else {
            reply.error(Errno::ENOENT);
            return;
        };
        match self
            .state
            .mutate(|writer| writer.remove_path(&path).map(|_| ()))
        {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(errno_for(&error)),
        }
    }

    fn rename(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        newparent: INodeNo,
        newname: &OsStr,
        _flags: fuser::RenameFlags,
        reply: ReplyEmpty,
    ) {
        if self.state.read_only() {
            reply.error(Errno::EROFS);
            return;
        }
        let (Some(name), Some(newname)) = (name.to_str(), newname.to_str()) else {
            reply.error(Errno::EINVAL);
            return;
        };
        let Some(old_path) = self.vfs().child_path(parent.into(), name) else {
            reply.error(Errno::ENOENT);
            return;
        };
        let Some(new_path) = self.vfs().child_path(newparent.into(), newname) else {
            reply.error(Errno::ENOENT);
            return;
        };
        if self.vfs().lookup(parent.into(), name).is_none() {
            reply.error(Errno::ENOENT);
            return;
        }
        let result = self.state.mutate(|writer| {
            // POSIX rename replaces an existing destination file.
            if let Some(dest) = writer.find_entry(&new_path) {
                if dest.is_directory {
                    return Err(SesameError::InvalidInput(format!(
                        "destination already exists: {new_path}"
                    )));
                }
                writer.remove_path(&new_path)?;
            }
            writer.rename_path(&old_path, &new_path)
        });
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(errno_for(&error)),
        }
    }

    fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        if flags.acc_mode() == OpenAccMode::O_RDONLY {
            reply.opened(FileHandle(0), FopenFlags::empty());
            return;
        }
        if self.state.read_only() {
            reply.error(Errno::EROFS);
            return;
        }
        let Some(metadata) = self.vfs().metadata(ino.into()) else {
            reply.error(Errno::ENOENT);
            return;
        };
        if metadata.kind == NodeKind::Directory {
            reply.error(Errno::EISDIR);
            return;
        }
        let Some(path) = self.vfs().path(ino.into()) else {
            reply.error(Errno::ENOENT);
            return;
        };
        let buffer = match self.vfs().read(ino.into(), 0, metadata.size as usize) {
            Ok(data) => data,
            Err(_) => {
                reply.error(Errno::EIO);
                return;
            }
        };
        let truncate = flags.0 & libc::O_TRUNC != 0;
        let fh = self.state.alloc_handle(OpenHandle {
            path,
            buffer,
            dirty: false,
        });
        if truncate {
            self.state.with_handle(fh, |handle| {
                handle.buffer.clear();
                handle.dirty = true;
            });
        }
        reply.opened(FileHandle(fh), FopenFlags::empty());
    }

    fn read(
        &self,
        _req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyData,
    ) {
        // Reads on a write handle see the uncommitted buffer.
        if fh.0 != 0
            && let Some(data) = self.state.with_handle(fh.0, |handle| {
                let start = (offset as usize).min(handle.buffer.len());
                let end = (start + size as usize).min(handle.buffer.len());
                handle.buffer[start..end].to_vec()
            })
        {
            reply.data(&data);
            return;
        }
        match self.vfs().read(ino.into(), offset, size as usize) {
            Ok(data) => reply.data(&data),
            Err(_) => reply.error(Errno::EIO),
        }
    }

    fn write(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        data: &[u8],
        _write_flags: fuser::WriteFlags,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        if self.state.read_only() {
            reply.error(Errno::EROFS);
            return;
        }
        let written = self.state.with_handle(fh.0, |handle| {
            let offset = offset as usize;
            if offset > handle.buffer.len() {
                handle.buffer.resize(offset, 0);
            }
            let end = offset + data.len();
            if end > handle.buffer.len() {
                handle.buffer.resize(end, 0);
            }
            handle.buffer[offset..end].copy_from_slice(data);
            handle.dirty = true;
            data.len() as u32
        });
        match written {
            Some(size) => reply.written(size),
            None => reply.error(Errno::EBADF),
        }
    }

    fn flush(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _lock_owner: LockOwner,
        reply: ReplyEmpty,
    ) {
        if fh.0 == 0 {
            reply.ok();
            return;
        }
        match self.state.commit_handle(fh.0) {
            Ok(()) => reply.ok(),
            Err(_) => reply.error(Errno::EIO),
        }
    }

    fn fsync(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        if fh.0 == 0 {
            reply.ok();
            return;
        }
        match self.state.commit_handle(fh.0) {
            Ok(()) => reply.ok(),
            Err(_) => reply.error(Errno::EIO),
        }
    }

    fn release(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        if fh.0 != 0 {
            // Last chance to commit; errors here cannot reach close().
            let _ = self.state.commit_handle(fh.0);
            self.state.take_handle(fh.0);
        }
        reply.ok();
    }

    fn create(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        if self.state.read_only() {
            reply.error(Errno::EROFS);
            return;
        }
        let Some(name) = name.to_str() else {
            reply.error(Errno::EINVAL);
            return;
        };
        if self.vfs().lookup(parent.into(), name).is_some() {
            reply.error(Errno::EEXIST);
            return;
        }
        let Some(path) = self.vfs().child_path(parent.into(), name) else {
            reply.error(Errno::ENOENT);
            return;
        };
        // Commit the empty file immediately so the entry exists; subsequent
        // writes flow through the open handle and commit on flush/release.
        if let Err(error) = self
            .state
            .mutate(|writer| writer.add_file_data(&[], &path, None, false).map(|_| ()))
        {
            reply.error(errno_for(&error));
            return;
        }
        let Some(attr) = self.attr_for_path(&path) else {
            reply.error(Errno::EIO);
            return;
        };
        let fh = self.state.alloc_handle(OpenHandle {
            path,
            buffer: Vec::new(),
            dirty: false,
        });
        reply.created(
            &TTL,
            &attr,
            Generation(0),
            FileHandle(fh),
            FopenFlags::empty(),
        );
    }

    fn readdir(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let Some(entries) = self.vfs().readdir(ino.into(), offset as usize) else {
            reply.error(Errno::ENOTDIR);
            return;
        };
        for (index, entry) in entries.into_iter().enumerate() {
            let kind = match entry.kind {
                NodeKind::Directory => FileType::Directory,
                NodeKind::File => FileType::RegularFile,
            };
            let next_offset = offset.saturating_add(index as u64).saturating_add(1);
            if reply.add(INodeNo(entry.inode), next_offset, kind, entry.name) {
                break;
            }
        }
        reply.ok();
    }
}

pub(super) fn prepare_mount_directory(mount_point: &Path) -> Result<()> {
    prepare_mount_directory_common(mount_point)
}

fn prepare_mount_directory_common(mount_point: &Path) -> Result<()> {
    std::fs::create_dir_all(mount_point).map_err(|error| {
        SesameError::Platform(format!(
            "cannot create mount point {}: {error}",
            mount_point.display()
        ))
    })?;
    if is_mount_point(mount_point)? {
        return Err(SesameError::InvalidInput(format!(
            "{}:{}",
            keys::MOUNT_POINT_BUSY,
            mount_point.display()
        )));
    }
    if std::fs::read_dir(mount_point)?.next().is_some() {
        return Err(SesameError::InvalidInput(format!(
            "mount point must be empty: {}",
            mount_point.display()
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn is_mount_point(path: &Path) -> Result<bool> {
    let metadata = path.metadata()?;
    let Some(parent) = path.parent() else {
        return Ok(false);
    };
    if parent.as_os_str().is_empty() {
        return Ok(false);
    }
    let parent_metadata = parent.metadata()?;
    Ok(metadata.dev() != parent_metadata.dev())
}

#[cfg(not(unix))]
fn is_mount_point(_path: &Path) -> Result<bool> {
    Ok(false)
}

fn mount_options(mount_point: &Path, read_only: bool) -> Vec<MountOption> {
    let mut options = vec![MountOption::DefaultPermissions];
    if read_only {
        options.push(MountOption::RO);
    }
    {
        let _ = mount_point;
        options.push(MountOption::FSName("closed-sesame".into()));
    }
    options
}

pub(super) fn spawn(state: Arc<FuseMountState>, mount_point: &Path) -> Result<BackgroundSession> {
    let mut config = Config::default();
    config.mount_options = mount_options(mount_point, state.read_only());
    fuser::spawn_mount2(FuseFilesystem::new(state), mount_point, &config)
        .map_err(|error| SesameError::Platform(format!("FUSE mount failed: {error}")))
}

fn unix_time(seconds: u64) -> SystemTime {
    UNIX_EPOCH
        .checked_add(Duration::from_secs(seconds))
        .unwrap_or(UNIX_EPOCH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn detects_mount_points_by_device_id() {
        let directory = TempDir::new().unwrap();
        let mount_point = directory.path().join("mnt");
        std::fs::create_dir(&mount_point).unwrap();
        assert!(!is_mount_point(&mount_point).unwrap());
    }
}
