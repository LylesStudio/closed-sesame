//! Diagnostic WinFsp harness independent of the ClosedSesame container and VFS.

#![cfg(all(target_os = "windows", feature = "mount"))]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;
use winfsp_wrs::{
    CreateFileInfo, DirInfo, FileAccessRights, FileAttributes, FileInfo, FileSystem,
    FileSystemInterface, NTSTATUS, PSecurityDescriptor, Params, STATUS_FILE_IS_A_DIRECTORY,
    STATUS_MEDIA_WRITE_PROTECTED, SecurityDescriptor, U16CStr, VolumeInfo, VolumeParams,
    filetime_now, u16cstr, u16str,
};

#[link(name = "delayimp")]
unsafe extern "C" {}

static GET_SECURITY_BY_NAME_CALLS: AtomicUsize = AtomicUsize::new(0);
static CREATE_EX_CALLS: AtomicUsize = AtomicUsize::new(0);
static OPEN_CALLS: AtomicUsize = AtomicUsize::new(0);
static READ_DIRECTORY_CALLS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
struct RootContext {
    info: FileInfo,
}

struct HarnessFs {
    root: RootContext,
    security_descriptor: SecurityDescriptor,
    volume_info: VolumeInfo,
}

impl HarnessFs {
    fn new() -> Self {
        let now = filetime_now();
        let mut root_info = FileInfo::default();
        root_info
            .set_file_attributes(FileAttributes::DIRECTORY | FileAttributes::READONLY)
            .set_time(now)
            .set_index_number(1);
        Self {
            root: RootContext { info: root_info },
            security_descriptor: SecurityDescriptor::from_wstr(u16cstr!(
                "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;WD)"
            ))
            .expect("valid diagnostic SDDL"),
            volume_info: VolumeInfo::new(4096, 4096, u16str!("WRSHARNESS"))
                .expect("valid volume label"),
        }
    }

    fn is_root(file_name: &U16CStr) -> bool {
        file_name
            .to_string_lossy()
            .trim_matches(['\\', '/'])
            .is_empty()
    }
}

impl FileSystemInterface for HarnessFs {
    type FileContext = Arc<RootContext>;

    const GET_VOLUME_INFO_DEFINED: bool = true;
    fn get_volume_info(&self) -> Result<VolumeInfo, NTSTATUS> {
        Ok(self.volume_info.clone())
    }

    const GET_SECURITY_BY_NAME_DEFINED: bool = true;
    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        _find_reparse_point: impl Fn() -> Option<FileAttributes>,
    ) -> Result<(FileAttributes, PSecurityDescriptor, bool), NTSTATUS> {
        GET_SECURITY_BY_NAME_CALLS.fetch_add(1, Ordering::SeqCst);
        if !Self::is_root(file_name) {
            return Err(winfsp_wrs::STATUS_OBJECT_NAME_NOT_FOUND);
        }
        Ok((
            self.root.info.file_attributes(),
            self.security_descriptor.as_ptr(),
            false,
        ))
    }

    const CREATE_EX_DEFINED: bool = true;
    fn create_ex(
        &self,
        _file_name: &U16CStr,
        _create_file_info: CreateFileInfo,
        _security_descriptor: SecurityDescriptor,
        _buffer: &[u8],
        _extra_buffer_is_reparse_point: bool,
    ) -> Result<(Self::FileContext, FileInfo), NTSTATUS> {
        CREATE_EX_CALLS.fetch_add(1, Ordering::SeqCst);
        Err(STATUS_MEDIA_WRITE_PROTECTED)
    }

    const OPEN_DEFINED: bool = true;
    fn open(
        &self,
        file_name: &U16CStr,
        _create_options: winfsp_wrs::CreateOptions,
        _granted_access: FileAccessRights,
    ) -> Result<(Self::FileContext, FileInfo), NTSTATUS> {
        OPEN_CALLS.fetch_add(1, Ordering::SeqCst);
        if !Self::is_root(file_name) {
            return Err(winfsp_wrs::STATUS_OBJECT_NAME_NOT_FOUND);
        }
        Ok((Arc::new(self.root.clone()), self.root.info))
    }

    const OVERWRITE_EX_DEFINED: bool = true;
    fn overwrite_ex(
        &self,
        _context: Self::FileContext,
        _file_attributes: FileAttributes,
        _replace_file_attributes: bool,
        _allocation_size: u64,
        _buffer: &[u8],
    ) -> Result<FileInfo, NTSTATUS> {
        Err(STATUS_MEDIA_WRITE_PROTECTED)
    }

    const CLOSE_DEFINED: bool = true;
    fn close(&self, _context: Self::FileContext) {}

    const GET_FILE_INFO_DEFINED: bool = true;
    fn get_file_info(&self, context: Self::FileContext) -> Result<FileInfo, NTSTATUS> {
        Ok(context.info)
    }

    const GET_SECURITY_DEFINED: bool = true;
    fn get_security(&self, _context: Self::FileContext) -> Result<PSecurityDescriptor, NTSTATUS> {
        Ok(self.security_descriptor.as_ptr())
    }

    const READ_DEFINED: bool = true;
    fn read(
        &self,
        _context: Self::FileContext,
        _buffer: &mut [u8],
        _offset: u64,
    ) -> Result<usize, NTSTATUS> {
        Err(STATUS_FILE_IS_A_DIRECTORY)
    }

    const READ_DIRECTORY_DEFINED: bool = true;
    fn read_directory(
        &self,
        _context: Self::FileContext,
        _marker: Option<&U16CStr>,
        _add_dir_info: impl FnMut(DirInfo) -> bool,
    ) -> Result<(), NTSTATUS> {
        READ_DIRECTORY_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum VolumeParamsProfile {
    Upstream,
    ReadOnly,
    Versioned,
    ClosedSesame,
}

fn params_for(profile: VolumeParamsProfile, mount_point: &U16CStr) -> Params {
    let mut volume_params = VolumeParams::default();
    volume_params
        .set_sector_size(512)
        .set_sectors_per_allocation_unit(1)
        .set_volume_creation_time(filetime_now())
        .set_volume_serial_number(0)
        .set_file_info_timeout(1000)
        .set_case_sensitive_search(true)
        .set_case_preserved_names(true)
        .set_unicode_on_disk(true)
        .set_persistent_acls(true)
        .set_post_cleanup_when_modified_only(true)
        .set_file_system_name(mount_point)
        .expect("valid filesystem name")
        .set_prefix(u16cstr!(""))
        .expect("valid prefix");

    match profile {
        VolumeParamsProfile::Upstream => {}
        VolumeParamsProfile::ReadOnly => {
            volume_params.set_read_only_volume(true);
        }
        VolumeParamsProfile::Versioned => {
            volume_params
                .set_read_only_volume(true)
                .set_version(504)
                .set_always_use_double_buffering(true);
        }
        VolumeParamsProfile::ClosedSesame => {
            volume_params
                .set_read_only_volume(true)
                .set_version(504)
                .set_always_use_double_buffering(true)
                .set_reparse_point(true)
                .set_reparse_point_access_check(false);
        }
    }
    Params {
        volume_params,
        ..Default::default()
    }
}

fn reset_counters() {
    for counter in [
        &GET_SECURITY_BY_NAME_CALLS,
        &CREATE_EX_CALLS,
        &OPEN_CALLS,
        &READ_DIRECTORY_CALLS,
    ] {
        counter.store(0, Ordering::SeqCst);
    }
}

fn print_counters(profile: VolumeParamsProfile, threaded: bool) {
    eprintln!(
        "profile={profile:?} threaded={threaded} security={} create_ex={} open={} read_directory={}",
        GET_SECURITY_BY_NAME_CALLS.load(Ordering::SeqCst),
        CREATE_EX_CALLS.load(Ordering::SeqCst),
        OPEN_CALLS.load(Ordering::SeqCst),
        READ_DIRECTORY_CALLS.load(Ordering::SeqCst),
    );
}

fn external_probe(drive: &str) -> Result<(), String> {
    std::fs::read_dir(PathBuf::from(format!("{drive}\\")))
        .map_err(|error| format!("in-process read_dir: {error}"))?;
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("Get-ChildItem -LiteralPath '{drive}\\' | Out-Null"),
        ])
        .output()
        .map_err(|error| format!("spawn PowerShell: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "PowerShell: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn run_profile(profile: VolumeParamsProfile, drive: &'static str, threaded: bool) -> bool {
    reset_counters();
    let mount_point = winfsp_wrs::U16CString::from_str(drive).expect("valid drive");
    if threaded {
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (stop_tx, stop_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = FileSystem::start(
                params_for(profile, &mount_point),
                Some(&mount_point),
                HarnessFs::new(),
            );
            match result {
                Ok(host) => {
                    ready_tx.send(Ok(())).ok();
                    stop_rx.recv().ok();
                    host.stop();
                }
                Err(status) => {
                    ready_tx.send(Err(status)).ok();
                }
            }
        });
        let started = ready_rx.recv_timeout(Duration::from_secs(10));
        let success = match started {
            Ok(Ok(())) => {
                std::thread::sleep(Duration::from_millis(300));
                external_probe(drive)
                    .map_err(|error| eprintln!("{error}"))
                    .is_ok()
            }
            Ok(Err(status)) => {
                eprintln!("FileSystem::start failed: {status:#x}");
                false
            }
            Err(error) => {
                eprintln!("worker did not start: {error}");
                false
            }
        };
        stop_tx.send(()).ok();
        worker.join().expect("harness worker panicked");
        print_counters(profile, threaded);
        success
    } else {
        let Ok(host) = FileSystem::start(
            params_for(profile, &mount_point),
            Some(&mount_point),
            HarnessFs::new(),
        ) else {
            eprintln!("FileSystem::start failed");
            return false;
        };
        std::thread::sleep(Duration::from_millis(300));
        let success = external_probe(drive)
            .map_err(|error| eprintln!("{error}"))
            .is_ok();
        host.stop();
        print_counters(profile, threaded);
        success
    }
}

#[test]
#[ignore = "requires an installed WinFsp runtime and free T:/U:/V:/W:/X: drive letters"]
fn volume_params_and_threading_matrix() {
    winfsp_wrs::init().expect("WinFsp runtime");
    let profiles = [
        (VolumeParamsProfile::Upstream, "T:"),
        (VolumeParamsProfile::ReadOnly, "U:"),
        (VolumeParamsProfile::Versioned, "V:"),
        (VolumeParamsProfile::ClosedSesame, "W:"),
    ];
    let mut results = Vec::new();
    for (profile, drive) in profiles {
        results.push((profile, false, run_profile(profile, drive, false)));
    }
    results.push((
        VolumeParamsProfile::Upstream,
        true,
        run_profile(VolumeParamsProfile::Upstream, "X:", true),
    ));
    eprintln!("harness results: {results:?}");
    assert!(
        results[0].2,
        "the exact upstream VolumeParams profile must be accessible"
    );
}
