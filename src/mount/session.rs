//! Opaque mounted-vault lifecycle.

use crate::error::Result;
#[cfg(target_os = "linux")]
use crate::error::SesameError;
use crate::keymgmt::KeyEncryptingKey;
use std::path::{Path, PathBuf};

pub(crate) enum BackendSession {
    #[cfg(target_os = "linux")]
    Fuse {
        session: Option<fuser::BackgroundSession>,
        state: std::sync::Arc<super::fuse_state::FuseMountState>,
    },
    WebDav(super::webdav::WebDavSession),
    #[cfg(target_os = "windows")]
    WinFsp(super::windows::WindowsSession),
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    Unsupported,
}

/// A live vault mount.
///
/// Calling [`unmount`](Self::unmount) is preferred; dropping the value performs
/// the same best-effort cleanup.
pub struct MountedVault {
    mount_point: PathBuf,
    backend: BackendSession,
}

impl MountedVault {
    pub(crate) fn new(mount_point: PathBuf, backend: BackendSession) -> Self {
        Self {
            mount_point,
            backend,
        }
    }

    /// Path or drive letter where the vault is mounted.
    pub fn mount_point(&self) -> &Path {
        &self.mount_point
    }

    /// Whether the backend session is still running.
    pub fn is_alive(&self) -> bool {
        match &self.backend {
            #[cfg(target_os = "linux")]
            BackendSession::Fuse { session, .. } => session
                .as_ref()
                .is_some_and(|session| !session.guard.is_finished()),
            BackendSession::WebDav(session) => session.is_alive(),
            #[cfg(target_os = "windows")]
            BackendSession::WinFsp(session) => session.is_alive(),
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            BackendSession::Unsupported => false,
        }
    }

    /// Reload the mounted filesystem view from the on-disk container.
    pub fn refresh_vault(&self, container: &Path, kek: &KeyEncryptingKey) -> Result<()> {
        match &self.backend {
            #[cfg(target_os = "windows")]
            BackendSession::WinFsp(session) => session.refresh_vfs(container, kek),
            #[cfg(target_os = "linux")]
            BackendSession::Fuse { state, .. } => {
                let _ = (container, kek);
                state.reload_vfs()
            }
            BackendSession::WebDav(session) => session.reload_vfs(),
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            BackendSession::Unsupported => Ok(()),
        }
    }

    /// Serialize an out-of-band container mutation (GUI actions, cache
    /// write-back) with FUSE-side commits. Returns `None` when the backend
    /// does not share state with this process (e.g. WinFsp, which mutates
    /// through its own session APIs).
    pub fn mutation_ticket(&self) -> Option<super::shared::MutationTicket> {
        match &self.backend {
            #[cfg(target_os = "linux")]
            BackendSession::Fuse { state, .. } => Some(state.ticket()),
            BackendSession::WebDav(session) => Some(session.ticket()),
            #[cfg(target_os = "windows")]
            BackendSession::WinFsp(session) => Some(session.ticket()),
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            BackendSession::Unsupported => None,
        }
    }

    pub fn mutation_lock(
        &self,
    ) -> Option<std::sync::MutexGuard<'_, crate::container::ContainerWriter>> {
        match &self.backend {
            #[cfg(target_os = "linux")]
            BackendSession::Fuse { state, .. } => Some(state.mutation_lock()),
            BackendSession::WebDav(session) => Some(session.mutation_lock()),
            #[cfg(target_os = "windows")]
            BackendSession::WinFsp(session) => Some(session.mutation_lock()),
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            BackendSession::Unsupported => None,
        }
    }

    /// Explicitly stop the backend and unmount the vault.
    ///
    /// This method is idempotent.
    pub fn unmount(&mut self) -> Result<()> {
        match &mut self.backend {
            #[cfg(target_os = "linux")]
            BackendSession::Fuse { session, .. } => {
                if let Some(session) = session.take() {
                    session.umount_and_join().map_err(|error| {
                        SesameError::Platform(format!("failed to unmount FUSE session: {error}"))
                    })?;
                }
                Ok(())
            }
            BackendSession::WebDav(session) => session.stop(),
            #[cfg(target_os = "windows")]
            BackendSession::WinFsp(session) => session.stop(),
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            BackendSession::Unsupported => Ok(()),
        }
    }
}

impl Drop for MountedVault {
    fn drop(&mut self) {
        let _ = self.unmount();
    }
}
