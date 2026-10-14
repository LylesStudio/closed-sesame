//! Cross-platform vault mounting (Linux FUSE, Windows WinFsp, macOS via the
//! built-in WebDAV bridge).
//!
//! Platform adapters are intentionally private. Call [`mount_vault`] and keep
//! the returned [`MountedVault`] alive for as long as the mount is needed.

mod deps;
mod session;
mod shared;
mod vfs;
#[cfg(target_os = "windows")]
mod write_buffer;

#[cfg(target_os = "linux")]
mod fuse_adapter;
#[cfg(target_os = "linux")]
mod fuse_state;
#[cfg(target_os = "linux")]
mod linux;
mod webdav;
#[cfg(target_os = "windows")]
mod windows;

use crate::app::credentials::{Credential, prepare_credentials};
use crate::error::{Result, SesameError};
use crate::keymgmt::KeyEncryptingKey;
use session::BackendSession;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use deps::{
    WINFSP_RELEASES_URL, platform_prerequisite_hint, platform_prerequisite_url,
    supplemental_guidance,
};
pub use session::MountedVault;
pub use shared::MutationTicket;

/// Whether mounting is available on the current platform build.
///
/// Every desktop platform has a mount path: FUSE on Linux, WinFsp on
/// Windows, and the built-in WebDAV bridge on macOS.
pub fn mount_supported() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "windows",
        target_os = "macos"
    ))
}

/// Verify platform mount prerequisites before opening the container.
pub fn check_prerequisites() -> Result<()> {
    deps::check_prerequisites()
}

/// Probe the WinFsp runtime once and cache the outcome (Windows only).
/// Used by the GUI to show an honest detection status instead of a static
/// "please install" hint for users who already have WinFsp.
#[cfg(target_os = "windows")]
pub fn winfsp_runtime_detected() -> bool {
    static DETECTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *DETECTED.get_or_init(|| windows::ensure_available().is_ok())
}

/// Owned credentials suitable for a mount that outlives CLI/UI input buffers.
pub enum MountCredentials {
    Password(String),
    NoPassword,
}

impl MountCredentials {
    fn as_credential(&self) -> Credential<'_> {
        match self {
            Self::Password(password) => Credential::Password(password),
            Self::NoPassword => Credential::NoPassword,
        }
    }
}

impl From<Credential<'_>> for MountCredentials {
    fn from(value: Credential<'_>) -> Self {
        match value {
            Credential::Password(password) => Self::Password(password.to_owned()),
            Credential::NoPassword => Self::NoPassword,
        }
    }
}

/// Mount a vault as a read-only filesystem.
///
/// The current metadata model supports hierarchical paths and directory entries.
pub fn mount_vault(
    credential: impl Into<MountCredentials>,
    container: impl AsRef<Path>,
    target: impl AsRef<Path>,
) -> Result<MountedVault> {
    let container = container.as_ref();
    let target = target.as_ref();
    if !container.is_file() {
        return Err(SesameError::InvalidInput(format!(
            "container is not a file: {}",
            container.display()
        )));
    }
    if target.as_os_str().is_empty() {
        return Err(SesameError::InvalidInput(
            "mount target cannot be empty".into(),
        ));
    }

    check_prerequisites()?;

    let credential = credential.into();
    let kek = prepare_credentials(container, credential.as_credential())?;
    mount_vault_with_kek(&kek, container, target)
}

/// Serve a vault over the loopback WebDAV bridge (no kernel filesystem).
///
/// This is the macOS mount path: Finder mounts WebDAV natively, so no
/// macFUSE/FSKit runtime is required. The returned [`MountedVault`]'s
/// `mount_point()` is the bridge URL (including its random session token).
pub fn mount_webdav_with_kek(
    kek: &KeyEncryptingKey,
    container: impl AsRef<Path>,
) -> Result<MountedVault> {
    let container = container.as_ref();
    if !container.is_file() {
        return Err(SesameError::InvalidInput(format!(
            "container is not a file: {}",
            container.display()
        )));
    }
    let shared = Arc::new(shared::SharedVault::open(container, kek)?);
    let session = webdav::WebDavSession::start(shared)?;
    let url = session.url().to_string();
    Ok(MountedVault::new(
        PathBuf::from(url),
        BackendSession::WebDav(session),
    ))
}

/// Mount an already-unlocked vault (for example from the GUI session).
pub fn mount_vault_with_kek(
    kek: &KeyEncryptingKey,
    container: impl AsRef<Path>,
    target: impl AsRef<Path>,
) -> Result<MountedVault> {
    let container = container.as_ref();
    let target = normalize_mount_target(target.as_ref());
    if !container.is_file() {
        return Err(SesameError::InvalidInput(format!(
            "container is not a file: {}",
            container.display()
        )));
    }
    if target.as_os_str().is_empty() {
        return Err(SesameError::InvalidInput(
            "mount target cannot be empty".into(),
        ));
    }
    check_prerequisites()?;

    #[cfg(target_os = "linux")]
    let state = Arc::new(fuse_state::FuseMountState::open(container, kek, false)?);
    #[cfg(target_os = "linux")]
    let backend = BackendSession::Fuse {
        session: Some(linux::mount(state.clone(), &target)?),
        state,
    };
    #[cfg(target_os = "macos")]
    let backend = {
        let _ = (container, &target);
        return Err(SesameError::Platform(
            crate::mount_hints::MACOS_MOUNT_DISABLED_MESSAGE.into(),
        ));
    };
    #[cfg(target_os = "windows")]
    let backend = {
        let shared = Arc::new(shared::SharedVault::open(container, kek)?);
        BackendSession::WinFsp(windows::mount(shared, &target)?)
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let backend = {
        let _ = vfs;
        return Err(SesameError::Platform(
            "mounting is unsupported on this operating system".into(),
        ));
    };

    Ok(MountedVault::new(target, backend))
}

fn normalize_mount_target(target: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        windows::normalize_mount_point(target)
    }
    #[cfg(not(target_os = "windows"))]
    {
        target.to_path_buf()
    }
}

/// Ask Finder to mount a WebDAV URL as a volume (macOS only; a no-op
/// elsewhere so callers can invoke it unconditionally).
pub fn finder_mount_volume(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("osascript")
            .args(["-e", &format!("mount volume \"{url}\"")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = url;
        false
    }
}

/// Returns true when `path` lies on or under the read-only mount root `mount`.
pub fn path_is_within_mount(path: &Path, mount: &Path) -> bool {
    #[cfg(target_os = "windows")]
    let mount = normalize_mount_target(mount);
    #[cfg(not(target_os = "windows"))]
    let mount = mount.to_path_buf();
    if mount.as_os_str().is_empty() || path.as_os_str().is_empty() {
        return false;
    }

    #[cfg(target_os = "windows")]
    {
        if is_windows_drive_mount(&mount) {
            return path_is_within_drive_mount(path, &mount);
        }
    }

    path_has_prefix(path, &mount)
}

#[cfg(target_os = "windows")]
fn is_windows_drive_mount(mount: &Path) -> bool {
    let text = mount.as_os_str().to_string_lossy();
    let bytes = text.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(target_os = "windows")]
fn path_is_within_drive_mount(path: &Path, mount: &Path) -> bool {
    let mount_drive = mount.as_os_str().to_string_lossy().to_ascii_uppercase();
    let path_text = path.as_os_str().to_string_lossy();
    let path_upper = path_text.to_ascii_uppercase();
    if path_upper == mount_drive {
        return true;
    }
    let prefix = format!("{}\\", &mount_drive[..2]);
    path_upper.starts_with(&prefix) || path_upper.starts_with(&format!("{}/", &mount_drive[..2]))
}

fn path_has_prefix(path: &Path, prefix: &Path) -> bool {
    let mut path_components = path.components();
    let mut prefix_components = prefix.components();
    loop {
        match (prefix_components.next(), path_components.next()) {
            (None, _) => return true,
            (Some(expected), Some(actual)) if components_equal(expected, actual) => {}
            _ => return false,
        }
    }
}

fn components_equal(left: std::path::Component<'_>, right: std::path::Component<'_>) -> bool {
    use std::path::Component;
    match (left, right) {
        // Case-insensitive only on Windows; Linux/macOS filesystems are
        // (by default) case-sensitive, and insensitive matching here would
        // misjudge whether an extract destination lies inside the mount.
        (Component::Normal(a), Component::Normal(b)) => {
            if cfg!(windows) {
                a.eq_ignore_ascii_case(b)
            } else {
                a == b
            }
        }
        (Component::Prefix(a), Component::Prefix(b)) => {
            a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
        }
        (Component::RootDir, Component::RootDir) => true,
        (Component::CurDir, Component::CurDir) => true,
        (a, b) => a.as_os_str() == b.as_os_str(),
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    #[cfg_attr(not(windows), ignore = "Windows-only drive letter paths")]
    fn path_is_within_drive_mount_matches_root_and_nested_paths() {
        let mount = PathBuf::from("Z:");
        assert!(path_is_within_mount(Path::new("Z:"), &mount));
        assert!(path_is_within_mount(
            Path::new("z:\\project\\file.txt"),
            &mount
        ));
        assert!(!path_is_within_mount(Path::new("C:\\project"), &mount));
    }

    #[test]
    fn path_is_within_directory_mount_matches_nested_paths() {
        let mount = PathBuf::from("/mnt/vault");
        assert!(path_is_within_mount(Path::new("/mnt/vault"), &mount));
        assert!(path_is_within_mount(
            Path::new("/mnt/vault/project/file.txt"),
            &mount
        ));
        assert!(!path_is_within_mount(Path::new("/mnt/other"), &mount));
    }
}
