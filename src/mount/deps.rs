//! Mount adapter hooks into shared runtime guidance.

pub use crate::mount_hints::{
    WINFSP_RELEASES_URL, platform_prerequisite_hint, platform_prerequisite_url,
    supplemental_guidance,
};

#[cfg(target_os = "windows")]
pub(crate) fn check_prerequisites() -> crate::error::Result<()> {
    super::windows::ensure_available()
}

#[cfg(target_os = "macos")]
pub(crate) fn check_prerequisites() -> crate::error::Result<()> {
    // macOS mounts via the loopback WebDAV bridge, which needs no external
    // runtime; there is nothing to validate up front.
    Ok(())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub(crate) fn check_prerequisites() -> crate::error::Result<()> {
    Ok(())
}
