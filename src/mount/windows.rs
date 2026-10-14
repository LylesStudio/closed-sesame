//! Windows writable filesystem backed by WinFsp.
//!
//! Reads are served from the VFS snapshot; writes are copy-on-write-on-close:
//! edits accumulate in a per-handle [`WriteBuffer`] and commit on
//! flush/cleanup/close through [`SharedVault`]'s two-phase commit, so an
//! interrupted write can never destroy the previous version of a file.

use super::shared::SharedVault;
use super::vfs::{NodeKind, ROOT_INODE, ReadOnlyVfs};
use super::write_buffer::WriteBuffer;
use crate::container::ContainerReader;
use crate::error::{Result, SesameError};
use crate::mount_hints::WINFSP_RELEASES_URL;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock, mpsc};
use std::thread::JoinHandle;
use winfsp_wrs::{
    CleanupFlags, CreateFileInfo, DirInfo, FileAccessRights, FileAttributes, FileInfo, FileSystem,
    FileSystemInterface, NTSTATUS, PSecurityDescriptor, Params, STATUS_ACCESS_DENIED,
    STATUS_CANNOT_DELETE, STATUS_DIRECTORY_NOT_EMPTY, STATUS_DISK_FULL, STATUS_END_OF_FILE,
    STATUS_FILE_IS_A_DIRECTORY, STATUS_NOT_A_DIRECTORY, STATUS_OBJECT_NAME_COLLISION,
    STATUS_OBJECT_NAME_NOT_FOUND, SecurityDescriptor, U16CStr, U16CString, VolumeInfo,
    VolumeParams, WriteMode, filetime_now,
};

const WINDOWS_EPOCH_OFFSET_SECS: u64 = 11_644_473_600;

#[derive(Debug, Clone, Copy)]
enum Handle {
    Root,
    File(u64),
}

/// Per-open-handle state. `buffer` is `Some` once the handle has written (or
/// been truncated) and holds the copy-on-write edits until commit.
struct FileContext {
    path: String,
    info: Mutex<FileInfo>,
    buffer: Mutex<Option<WriteBuffer>>,
    pending_delete: Mutex<bool>,
}

impl FileContext {
    fn new(path: String, info: FileInfo) -> Self {
        Self {
            path,
            info: Mutex::new(info),
            buffer: Mutex::new(None),
            pending_delete: Mutex::new(false),
        }
    }

    fn info(&self) -> FileInfo {
        self.info
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn set_size(&self, size: u64) {
        let mut info = self
            .info
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        info.set_file_size(size)
            .set_allocation_size(size.div_ceil(4096) * 4096)
            .set_last_write_time(filetime_now());
    }
}

struct WindowsFilesystem {
    shared: Arc<SharedVault>,
    security_descriptor: SecurityDescriptor,
    volume_info: VolumeInfo,
}

fn status_for(error: &SesameError) -> NTSTATUS {
    match error {
        SesameError::DiskFull { .. } => STATUS_DISK_FULL,
        SesameError::InvalidInput(message) => {
            if message.contains("not found") {
                STATUS_OBJECT_NAME_NOT_FOUND
            } else if message.contains("already exists") {
                STATUS_OBJECT_NAME_COLLISION
            } else {
                STATUS_ACCESS_DENIED
            }
        }
        _ => STATUS_ACCESS_DENIED,
    }
}

impl WindowsFilesystem {
    fn vfs(&self) -> std::sync::RwLockReadGuard<'_, ReadOnlyVfs<ContainerReader>> {
        self.shared.vfs()
    }

    fn resolve(&self, file_name: &U16CStr) -> std::result::Result<Handle, NTSTATUS> {
        let path = file_name.to_string_lossy();
        let path = path.trim_start_matches('\\');
        if path.is_empty() {
            return Ok(Handle::Root);
        }
        let vfs = self.vfs();
        let mut inode = ROOT_INODE;
        for component in path.split(['\\', '/']).filter(|part| !part.is_empty()) {
            inode = vfs
                .lookup(inode, component)
                .ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
        }
        Ok(Handle::File(inode))
    }

    fn resolve_path(&self, file_name: &U16CStr) -> String {
        file_name
            .to_string_lossy()
            .trim_start_matches('\\')
            .replace('\\', "/")
    }

    fn file_info(&self, handle: Handle) -> std::result::Result<FileInfo, NTSTATUS> {
        let inode = match handle {
            Handle::Root => ROOT_INODE,
            Handle::File(inode) => inode,
        };
        let vfs = self.vfs();
        let metadata = vfs.metadata(inode).ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
        let directory = metadata.kind == NodeKind::Directory;
        let mut info = FileInfo::default();
        info.set_file_attributes(if directory {
            FileAttributes::DIRECTORY
        } else {
            FileAttributes::NORMAL
        })
        .set_allocation_size(if directory {
            0
        } else {
            metadata.size.div_ceil(4096) * 4096
        })
        .set_file_size(metadata.size)
        .set_creation_time(windows_filetime(metadata.created_at))
        .set_last_access_time(windows_filetime(metadata.modified_at))
        .set_last_write_time(windows_filetime(metadata.modified_at))
        .set_change_time(windows_filetime(metadata.modified_at))
        .set_index_number(metadata.inode);
        Ok(info)
    }

    fn open_context(&self, handle: Handle) -> std::result::Result<Arc<FileContext>, NTSTATUS> {
        let inode = match handle {
            Handle::Root => ROOT_INODE,
            Handle::File(inode) => inode,
        };
        let path = {
            let vfs = self.vfs();
            if inode == ROOT_INODE {
                String::new()
            } else {
                vfs.path(inode).ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?
            }
        };
        let info = self.file_info(handle)?;
        Ok(Arc::new(FileContext::new(path, info)))
    }

    /// Initialize the write buffer from the vault content on first write.
    fn ensure_buffer(&self, context: &FileContext) -> std::result::Result<(), NTSTATUS> {
        let mut buffer = context
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if buffer.is_none() {
            let data = self
                .shared
                .read_file(&context.path)
                .map_err(|_| STATUS_OBJECT_NAME_NOT_FOUND)?;
            *buffer = Some(WriteBuffer::from_bytes(data));
        }
        Ok(())
    }

    /// Commit a dirty buffer into the container (two-phase commit + reload).
    fn commit(&self, context: &FileContext) -> std::result::Result<(), NTSTATUS> {
        let pending = {
            let buffer = context
                .buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            buffer.as_ref().and_then(|buffer| {
                if buffer.is_dirty() {
                    Some(buffer.data().to_vec())
                } else {
                    None
                }
            })
        };
        let Some(data) = pending else {
            return Ok(());
        };
        self.shared
            .write_file(&context.path, &data)
            .map_err(|error| status_for(&error))?;
        if let Some(buffer) = context
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_mut()
        {
            buffer.mark_clean();
        }
        Ok(())
    }

    fn refresh_info_from_vault(
        &self,
        context: &FileContext,
    ) -> std::result::Result<FileInfo, NTSTATUS> {
        let vfs = self.vfs();
        let inode = vfs
            .lookup_path(&context.path)
            .ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
        let metadata = vfs.metadata(inode).ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
        context.set_size(metadata.size);
        Ok(context.info())
    }
}

impl FileSystemInterface for WindowsFilesystem {
    type FileContext = Arc<FileContext>;

    const GET_SECURITY_BY_NAME_DEFINED: bool = true;
    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        _find_reparse_point: impl Fn() -> Option<FileAttributes>,
    ) -> std::result::Result<(FileAttributes, PSecurityDescriptor, bool), NTSTATUS> {
        let handle = self.resolve(file_name)?;
        Ok((
            self.file_info(handle)?.file_attributes(),
            self.security_descriptor.as_ptr(),
            false,
        ))
    }

    const CREATE_EX_DEFINED: bool = true;
    fn create_ex(
        &self,
        file_name: &U16CStr,
        create_file_info: CreateFileInfo,
        _security_descriptor: SecurityDescriptor,
        _buffer: &[u8],
        _extra_buffer_is_reparse_point: bool,
    ) -> std::result::Result<(Self::FileContext, FileInfo), NTSTATUS> {
        let path = self.resolve_path(file_name);
        if path.is_empty() {
            return Err(STATUS_ACCESS_DENIED);
        }
        if self.shared.metadata_of(&path).is_some() {
            return Err(STATUS_OBJECT_NAME_COLLISION);
        }
        let is_directory = create_file_info
            .create_options
            .is(winfsp_wrs::CreateOptions::FILE_DIRECTORY_FILE);
        if is_directory {
            self.shared
                .mkdir(&path)
                .map_err(|error| status_for(&error))?;
        } else {
            self.shared
                .write_file(&path, &[])
                .map_err(|error| status_for(&error))?;
        }
        let context = self
            .open_context(self.resolve(file_name)?)
            .map_err(|_| STATUS_OBJECT_NAME_NOT_FOUND)?;
        if !is_directory {
            // Fresh file: start with an empty buffer so writes never re-read.
            *context
                .buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(WriteBuffer::empty());
        }
        let info = context.info();
        Ok((context, info))
    }

    const OPEN_DEFINED: bool = true;
    fn open(
        &self,
        file_name: &U16CStr,
        _create_options: winfsp_wrs::CreateOptions,
        _granted_access: FileAccessRights,
    ) -> std::result::Result<(Self::FileContext, FileInfo), NTSTATUS> {
        let context = self.open_context(self.resolve(file_name)?)?;
        let info = context.info();
        Ok((context, info))
    }

    // WinFsp rejects every Create IRP before dispatch unless an overwrite callback is registered,
    // even for requests that only open existing files.
    const OVERWRITE_EX_DEFINED: bool = true;
    fn overwrite_ex(
        &self,
        context: Self::FileContext,
        _file_attributes: FileAttributes,
        _replace_file_attributes: bool,
        _allocation_size: u64,
        _buffer: &[u8],
    ) -> std::result::Result<FileInfo, NTSTATUS> {
        // Truncate-on-open: the next commit stores an empty file.
        {
            let mut buffer = context
                .buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut fresh = WriteBuffer::empty();
            fresh.resize(0);
            *buffer = Some(fresh);
        }
        context.set_size(0);
        Ok(context.info())
    }

    const CLEANUP_DEFINED: bool = true;
    fn cleanup(
        &self,
        context: Self::FileContext,
        _file_name: Option<&U16CStr>,
        flags: CleanupFlags,
    ) {
        // Last user handle is gone. Windows may have held the bytes in the
        // cache manager until this point; FlushAndPurgeOnCleanup has already
        // pushed them into the write buffer. Commit here, because Close can
        // be delayed until after unmount and would drop the buffer.
        if flags.is(CleanupFlags::DELETE) {
            let pending = *context
                .pending_delete
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if pending && !context.path.is_empty() {
                if let Some(buffer) = context
                    .buffer
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .as_mut()
                {
                    buffer.mark_clean();
                }
                let _ = self.shared.remove(&context.path);
                return;
            }
        }
        let _ = self.commit(&context);
    }

    const CLOSE_DEFINED: bool = true;
    fn close(&self, context: Self::FileContext) {
        // Last chance to commit; errors cannot reach the caller of CloseHandle.
        let _ = self.commit(&context);
    }

    const READ_DEFINED: bool = true;
    fn read(
        &self,
        context: Self::FileContext,
        buffer: &mut [u8],
        offset: u64,
    ) -> std::result::Result<usize, NTSTATUS> {
        // Reads on a written handle see the uncommitted buffer.
        {
            let write_buffer = context
                .buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(write_buffer) = write_buffer.as_ref() {
                let data = write_buffer.read_at(offset, buffer.len());
                buffer[..data.len()].copy_from_slice(data);
                return Ok(data.len());
            }
        }
        if context.path.is_empty() {
            return Err(STATUS_FILE_IS_A_DIRECTORY);
        }
        let vfs = self.vfs();
        let inode = vfs
            .lookup_path(&context.path)
            .ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
        let metadata = vfs.metadata(inode).ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
        if metadata.kind == NodeKind::Directory {
            return Err(STATUS_FILE_IS_A_DIRECTORY);
        }
        if offset >= metadata.size {
            return Err(STATUS_END_OF_FILE);
        }
        let data = vfs
            .read(inode, offset, buffer.len())
            .map_err(|_| STATUS_OBJECT_NAME_NOT_FOUND)?;
        buffer[..data.len()].copy_from_slice(&data);
        Ok(data.len())
    }

    const WRITE_DEFINED: bool = true;
    fn write(
        &self,
        context: Self::FileContext,
        buffer: &[u8],
        mode: WriteMode,
    ) -> std::result::Result<(usize, FileInfo), NTSTATUS> {
        if context.path.is_empty() {
            return Err(STATUS_FILE_IS_A_DIRECTORY);
        }
        self.ensure_buffer(&context)?;
        let written = {
            let mut write_buffer = context
                .buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let write_buffer = write_buffer.as_mut().ok_or(STATUS_ACCESS_DENIED)?;
            match mode {
                WriteMode::Normal { offset } => {
                    write_buffer.write_at(offset, buffer);
                    buffer.len()
                }
                WriteMode::ConstrainedIO { offset } => {
                    write_buffer.write_constrained(offset, buffer)
                }
                WriteMode::WriteToEOF => {
                    write_buffer.append(buffer);
                    buffer.len()
                }
            }
        };
        let len = context
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(WriteBuffer::len)
            .unwrap_or(0);
        context.set_size(len);
        Ok((written, context.info()))
    }

    const FLUSH_DEFINED: bool = true;
    fn flush(&self, context: Self::FileContext) -> std::result::Result<FileInfo, NTSTATUS> {
        self.commit(&context)?;
        self.refresh_info_from_vault(&context)
    }

    const GET_FILE_INFO_DEFINED: bool = true;
    fn get_file_info(&self, context: Self::FileContext) -> std::result::Result<FileInfo, NTSTATUS> {
        Ok(context.info())
    }

    const SET_BASIC_INFO_DEFINED: bool = true;
    fn set_basic_info(
        &self,
        context: Self::FileContext,
        _file_attributes: FileAttributes,
        _creation_time: u64,
        _last_access_time: u64,
        _last_write_time: u64,
        _change_time: u64,
    ) -> std::result::Result<FileInfo, NTSTATUS> {
        // Attribute/timestamp changes are accepted and ignored: the vault
        // owns its own metadata model.
        Ok(context.info())
    }

    const SET_FILE_SIZE_DEFINED: bool = true;
    fn set_file_size(
        &self,
        context: Self::FileContext,
        new_size: u64,
        _set_allocation_size: bool,
    ) -> std::result::Result<FileInfo, NTSTATUS> {
        if context.path.is_empty() {
            return Err(STATUS_FILE_IS_A_DIRECTORY);
        }
        self.ensure_buffer(&context)?;
        {
            let mut write_buffer = context
                .buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let write_buffer = write_buffer.as_mut().ok_or(STATUS_ACCESS_DENIED)?;
            write_buffer.resize(new_size);
        }
        context.set_size(new_size);
        Ok(context.info())
    }

    const SET_DELETE_DEFINED: bool = true;
    fn set_delete(
        &self,
        context: Self::FileContext,
        _file_name: &U16CStr,
        delete_file: bool,
    ) -> std::result::Result<(), NTSTATUS> {
        if !delete_file {
            *context
                .pending_delete
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = false;
            return Ok(());
        }
        if context.path.is_empty() {
            return Err(STATUS_CANNOT_DELETE);
        }
        let vfs = self.vfs();
        let inode = vfs
            .lookup_path(&context.path)
            .ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
        let metadata = vfs.metadata(inode).ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
        if metadata.kind == NodeKind::Directory {
            let has_children = self
                .shared
                .list_dir(&context.path)
                .map(|children| !children.is_empty())
                .unwrap_or(false);
            if has_children {
                return Err(STATUS_DIRECTORY_NOT_EMPTY);
            }
        }
        *context
            .pending_delete
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        Ok(())
    }

    const RENAME_DEFINED: bool = true;
    fn rename(
        &self,
        context: Self::FileContext,
        _file_name: &U16CStr,
        new_file_name: &U16CStr,
        replace_if_exists: bool,
    ) -> std::result::Result<(), NTSTATUS> {
        if context.path.is_empty() {
            return Err(STATUS_ACCESS_DENIED);
        }
        let new_path = self.resolve_path(new_file_name);
        if new_path.is_empty() {
            return Err(STATUS_ACCESS_DENIED);
        }
        if self.shared.metadata_of(&new_path).is_some() && !replace_if_exists {
            return Err(STATUS_OBJECT_NAME_COLLISION);
        }
        self.shared
            .rename(&context.path, &new_path, replace_if_exists)
            .map_err(|error| status_for(&error))
    }

    const GET_SECURITY_DEFINED: bool = true;
    fn get_security(
        &self,
        context: Self::FileContext,
    ) -> std::result::Result<PSecurityDescriptor, NTSTATUS> {
        let _ = context;
        Ok(self.security_descriptor.as_ptr())
    }

    const READ_DIRECTORY_DEFINED: bool = true;
    fn read_directory(
        &self,
        context: Self::FileContext,
        marker: Option<&U16CStr>,
        mut add_dir_info: impl FnMut(DirInfo) -> bool,
    ) -> std::result::Result<(), NTSTATUS> {
        let inode = {
            let vfs = self.vfs();
            if context.path.is_empty() {
                ROOT_INODE
            } else {
                vfs.lookup_path(&context.path)
                    .ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?
            }
        };
        {
            let vfs = self.vfs();
            let metadata = vfs.metadata(inode).ok_or(STATUS_OBJECT_NAME_NOT_FOUND)?;
            if metadata.kind != NodeKind::Directory {
                return Err(STATUS_NOT_A_DIRECTORY);
            }
        }

        let marker = marker.map(U16CStr::to_string_lossy);
        let vfs = self.vfs();
        for entry in vfs
            .readdir(inode, 0)
            .ok_or(STATUS_NOT_A_DIRECTORY)?
            .into_iter()
            .filter(|entry| entry.name != "." && entry.name != "..")
        {
            if marker.as_ref().is_some_and(|marker| entry.name <= *marker) {
                continue;
            }
            let info = DirInfo::from_str(self.file_info(Handle::File(entry.inode))?, &entry.name);
            if !add_dir_info(info) {
                break;
            }
        }
        Ok(())
    }

    const GET_VOLUME_INFO_DEFINED: bool = true;
    fn get_volume_info(&self) -> std::result::Result<VolumeInfo, NTSTATUS> {
        Ok(self.volume_info.clone())
    }
}

pub(crate) struct WindowsSession {
    stop_sender: Option<mpsc::Sender<()>>,
    worker: Option<JoinHandle<()>>,
    alive: Arc<AtomicBool>,
    shared: Arc<SharedVault>,
}

impl WindowsSession {
    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
            && self
                .worker
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
    }

    pub(crate) fn refresh_vfs(
        &self,
        container: &Path,
        kek: &crate::keymgmt::KeyEncryptingKey,
    ) -> Result<()> {
        let _ = (container, kek);
        self.shared.reload_vfs()
    }

    pub(crate) fn mutation_lock(
        &self,
    ) -> std::sync::MutexGuard<'_, crate::container::ContainerWriter> {
        self.shared.mutation_lock()
    }

    pub(crate) fn ticket(&self) -> super::shared::MutationTicket {
        super::shared::MutationTicket::new(std::sync::Arc::clone(&self.shared))
    }

    pub(crate) fn stop(&mut self) -> Result<()> {
        if let Some(sender) = self.stop_sender.take() {
            let _ = sender.send(());
        }
        if let Some(worker) = self.worker.take() {
            worker.join().map_err(|_| {
                SesameError::Platform("WinFsp host thread panicked while stopping".into())
            })?;
        }
        self.alive.store(false, Ordering::Release);
        Ok(())
    }
}

impl Drop for WindowsSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub(super) fn mount(shared: Arc<SharedVault>, mount_point: &Path) -> Result<WindowsSession> {
    winfsp_wrs::init().map_err(winfsp_init_error)?;
    prepare_mount_point(mount_point)?;

    let target = normalize_mount_point(mount_point);
    let alive = Arc::new(AtomicBool::new(false));
    let thread_alive = Arc::clone(&alive);
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let (stop_sender, stop_receiver) = mpsc::channel();
    let thread_shared = Arc::clone(&shared);
    let worker = std::thread::Builder::new()
        .name("closed-sesame-winfsp".into())
        .spawn(move || match start_host(thread_shared, &target) {
            Ok(host) => {
                thread_alive.store(true, Ordering::Release);
                if ready_sender.send(Ok(())).is_err() {
                    host.stop();
                    return;
                }
                let _ = stop_receiver.recv();
                thread_alive.store(false, Ordering::Release);
                host.stop();
            }
            Err(error) => {
                let _ = ready_sender.send(Err(error));
            }
        })
        .map_err(|error| SesameError::Platform(format!("cannot start WinFsp host: {error}")))?;

    ready_receiver.recv().map_err(|error| {
        SesameError::Platform(format!("WinFsp host thread exited unexpectedly: {error}"))
    })??;

    Ok(WindowsSession {
        stop_sender: Some(stop_sender),
        worker: Some(worker),
        alive,
        shared,
    })
}

fn start_host(shared: Arc<SharedVault>, target: &Path) -> Result<FileSystem> {
    let user_target = normalize_mount_point(target);
    let mount_point = winfsp_mount_point(&user_target)?;
    let (params, filesystem) = build_host(shared, &mount_point)?;
    let host =
        FileSystem::start(params, Some(&mount_point), filesystem).map_err(winfsp_start_error)?;
    std::thread::sleep(std::time::Duration::from_millis(300));
    verify_mount_accessible(&user_target)?;
    Ok(host)
}

fn build_host(
    shared: Arc<SharedVault>,
    mount_point: &U16CStr,
) -> Result<(Params, WindowsFilesystem)> {
    let total_size = shared.vfs().total_file_size().max(4096);
    let volume_info = VolumeInfo::new(total_size, total_size, winfsp_wrs::u16str!("ClosedSesame"))
        .map_err(|_| SesameError::Platform("WinFsp volume label is too long".into()))?;
    let mut volume_params = VolumeParams::default();
    volume_params
        .set_version(504)
        .set_always_use_double_buffering(true)
        .set_sector_size(512)
        .set_sectors_per_allocation_unit(1)
        .set_volume_creation_time(filetime_now())
        .set_volume_serial_number(0)
        // 0 disables the kernel FileInfo cache so a handle opened immediately
        // after a commit sees the new size. Flush-on-cleanup forces cached
        // writes into our copy-on-write buffer before the last handle closes.
        .set_file_info_timeout(0)
        .set_case_sensitive_search(true)
        .set_case_preserved_names(true)
        .set_unicode_on_disk(true)
        .set_persistent_acls(true)
        .set_flush_and_purge_on_cleanup(true)
        .set_post_cleanup_when_modified_only(true)
        .set_file_system_name(mount_point)
        .map_err(|_| SesameError::Platform("WinFsp filesystem name is too long".into()))?;
    volume_params
        .set_prefix(winfsp_wrs::u16cstr!(""))
        .map_err(|_| SesameError::Platform("WinFsp volume prefix is too long".into()))?;
    let params = Params {
        volume_params,
        ..Default::default()
    };
    let security_descriptor = SecurityDescriptor::from_wstr(winfsp_wrs::u16cstr!(
        "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)"
    ))
    .map_err(|_| SesameError::Platform("invalid WinFsp security descriptor".into()))?;
    Ok((
        params,
        WindowsFilesystem {
            shared,
            security_descriptor,
            volume_info,
        },
    ))
}

fn verify_mount_accessible(target: &Path) -> Result<()> {
    let probe = mount_probe_path(target);
    std::fs::read_dir(&probe).map_err(|error| {
        SesameError::Platform(format!(
            "WinFsp mount point is not accessible ({}): {error}",
            probe.display()
        ))
    })?;

    if is_drive_letter(target) {
        verify_mount_accessible_externally(target)?;
    }
    Ok(())
}

fn mount_probe_path(target: &Path) -> PathBuf {
    if is_drive_letter(target) {
        PathBuf::from(format!("{}\\", target.to_string_lossy()))
    } else {
        target.to_path_buf()
    }
}

/// Opens the drive root from a child process: an in-process `read_dir` can succeed
/// while other processes still cannot see the volume. `cmd` starts in tens of
/// milliseconds, where a cold PowerShell start took several seconds.
fn verify_mount_accessible_externally(target: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let drive = target.to_string_lossy();
    let listing = std::process::Command::new("cmd")
        // `dir` exits non-zero on an empty volume; `cd` only needs the root.
        .args(["/d", "/c", "cd", "/d", &format!("{drive}\\")])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| {
            SesameError::Platform(format!(
                "cannot verify mount point {} from an external process: {error}",
                target.display()
            ))
        })?;
    if listing.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&listing.stderr);
    let stdout = String::from_utf8_lossy(&listing.stdout);
    Err(SesameError::Platform(format!(
        "mount point {} is not accessible from an external process: {stdout}{stderr}",
        target.display()
    )))
}

fn winfsp_start_error(status: NTSTATUS) -> SesameError {
    match status as u32 {
        0xC000_0035 => SesameError::Platform(
            "drive letter is already in use; try another letter or restart WinFsp.Launcher".into(),
        ),
        0xC000_0022 => SesameError::Platform(
            "WinFsp denied access to the mount point; do not run as administrator unless required, and confirm WinFsp 2.x (Core + Developer) is installed".into(),
        ),
        _ => SesameError::Platform(format!("cannot start WinFsp host: {status:#x}")),
    }
}

/// WinFsp mount path. Drive letters use the same `X:` form as the official memfs sample.
fn winfsp_mount_point(user_target: &Path) -> Result<U16CString> {
    let user_target = normalize_mount_point(user_target);
    U16CString::from_os_str(user_target.as_os_str())
        .map_err(|error| SesameError::Platform(format!("invalid WinFsp mount path: {error}")))
}

fn prepare_mount_point(mount_point: &Path) -> Result<()> {
    if is_drive_letter(mount_point) {
        return Ok(());
    }
    std::fs::create_dir_all(mount_point).map_err(|error| {
        SesameError::Platform(format!(
            "cannot create mount point {}: {error}",
            mount_point.display()
        ))
    })?;
    if std::fs::read_dir(mount_point)?.next().is_some() {
        return Err(SesameError::InvalidInput(format!(
            "mount point must be empty: {}",
            mount_point.display()
        )));
    }
    Ok(())
}

pub(super) fn normalize_mount_point(path: &Path) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    let bytes = text.as_bytes();
    if bytes.len() == 6
        && bytes[0] == b'\\'
        && bytes[1] == b'\\'
        && bytes[2] == b'?'
        && bytes[3] == b'\\'
        && bytes[4].is_ascii_alphabetic()
        && bytes[5] == b':'
    {
        let drive = (bytes[4] as char).to_ascii_uppercase();
        return PathBuf::from(format!("{drive}:"));
    }
    if (bytes.len() == 2 && bytes[1] == b':')
        || (bytes.len() == 3 && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/'))
    {
        let drive = (bytes[0] as char).to_ascii_uppercase();
        if drive.is_ascii_alphabetic() {
            return PathBuf::from(format!("{drive}:"));
        }
    }
    path.to_path_buf()
}

fn is_drive_letter(path: &Path) -> bool {
    let text = path.as_os_str().to_string_lossy();
    let bytes = text.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn winfsp_init_error(error: winfsp_wrs::InitError) -> SesameError {
    SesameError::Platform(format!(
        "WinFsp runtime is required for mounting; install WinFsp 2.x (Core component) from {WINFSP_RELEASES_URL}: {error}"
    ))
}

pub(super) fn ensure_available() -> Result<()> {
    winfsp_wrs::init().map_err(winfsp_init_error)
}

fn windows_filetime(unix_seconds: u64) -> u64 {
    unix_seconds
        .saturating_add(WINDOWS_EPOCH_OFFSET_SECS)
        .saturating_mul(10_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_only_bare_drive_letters() {
        assert!(is_drive_letter(Path::new("X:")));
        assert!(is_drive_letter(Path::new("z:")));
        assert!(!is_drive_letter(Path::new("X:\\")));
        assert!(!is_drive_letter(Path::new("mount")));
    }

    #[test]
    fn normalizes_drive_mount_points() {
        assert_eq!(normalize_mount_point(Path::new("x:")), PathBuf::from("X:"));
        assert_eq!(
            normalize_mount_point(Path::new("Z:\\")),
            PathBuf::from("Z:")
        );
        assert_eq!(normalize_mount_point(Path::new("z:/")), PathBuf::from("Z:"));
        assert_eq!(
            normalize_mount_point(Path::new("\\\\?\\q:")),
            PathBuf::from("Q:")
        );
    }
}
