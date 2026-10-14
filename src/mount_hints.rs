//! User-facing mount runtime guidance (no platform FFI).
//!
//! URLs and error formatting live here so CLI, GUI, and the mount adapter share
//! one source of truth without requiring the `mount` feature for basic hints.

use crate::error::SesameError;

/// Official WinFsp release page for Windows users.
pub const WINFSP_RELEASES_URL: &str = "https://github.com/winfsp/winfsp/releases";

/// Error text for the (removed) macOS FUSE mount path: macOS mounts via the
/// built-in WebDAV bridge instead.
pub const MACOS_MOUNT_DISABLED_MESSAGE: &str = "macOS FUSE mounting is unavailable; use the built-in WebDAV bridge (GUI Mount, or `sesame mount --webdav`)";

/// Short install guidance for the current target OS.
pub fn platform_prerequisite_hint() -> String {
    #[cfg(target_os = "windows")]
    {
        ts!("mount.hint.windows")
    }
    #[cfg(target_os = "macos")]
    {
        ts!("mount.hint.macos_webdav")
    }
    #[cfg(target_os = "linux")]
    {
        ts!("mount.hint.linux")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        ts!("mount.hint.unsupported")
    }
}

/// Download URL for the current platform, when one exists.
pub fn platform_prerequisite_url() -> Option<&'static str> {
    #[cfg(target_os = "windows")]
    {
        Some(WINFSP_RELEASES_URL)
    }
    #[cfg(target_os = "macos")]
    {
        None
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

/// Whether a platform error likely means the mount runtime is missing.
pub fn is_missing_runtime_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("winfsp") || lower.contains("/dev/fuse")
}

fn is_mount_conflict_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("file exists")
        || lower.contains("eexist")
        || lower.contains("already mounted")
        || lower.contains("mount point")
}

/// Extra lines to print after a mount failure (CLI stderr or GUI status).
pub fn supplemental_guidance(error: &SesameError) -> Option<String> {
    match error {
        SesameError::Platform(message) if message == MACOS_MOUNT_DISABLED_MESSAGE => {
            Some(platform_prerequisite_hint())
        }
        SesameError::Platform(message) if is_missing_runtime_message(message) => {
            let mut lines = vec![platform_prerequisite_hint()];
            // Pick the download URL that matches the failing runtime, not
            // just the host platform.
            let lower = message.to_ascii_lowercase();
            let url = if lower.contains("winfsp") {
                Some(WINFSP_RELEASES_URL)
            } else if lower.contains("/dev/fuse") {
                // FUSE comes from the distro's package manager; pointing a
                // Linux user at the WinFsp page (the host default on
                // Windows) would be wrong.
                None
            } else {
                platform_prerequisite_url()
            };
            if let Some(url) = url {
                lines.push(format!("{}: {url}", ts!("common.download")));
            }
            Some(lines.join("\n"))
        }
        SesameError::Platform(message) if is_mount_conflict_message(message) => {
            let _ = message;
            None
        }
        SesameError::InvalidInput(message) if message.starts_with("error.mount_point_busy:") => {
            Some(ts!("mount.hint.unmount_stale"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Locale, init};

    #[test]
    fn detects_winfsp_runtime_errors() {
        assert!(is_missing_runtime_message("WinFsp runtime is required"));
        assert!(!is_missing_runtime_message("permission denied"));
    }

    #[test]
    #[cfg_attr(not(windows), ignore = "WinFsp URL only appears in Windows guidance")]
    fn platform_error_includes_official_releases_url() {
        init(Some(Locale::En));
        let error = SesameError::Platform("WinFsp runtime is required".into());
        let guidance = supplemental_guidance(&error).expect("guidance");
        assert!(guidance.contains(WINFSP_RELEASES_URL));
    }

    #[test]
    fn missing_fuse_device_guidance_has_no_download_url() {
        init(Some(Locale::En));
        let error = SesameError::Platform("/dev/fuse is unavailable".into());
        let guidance = supplemental_guidance(&error).expect("guidance");
        assert!(!guidance.contains(ts!("common.download").as_str()));
    }
}
