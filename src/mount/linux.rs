//! Linux FUSE availability checks and mounting.

use super::fuse_adapter;
use super::fuse_state::FuseMountState;
use crate::error::{Result, SesameError};
use fuser::BackgroundSession;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::sync::Arc;

pub(super) fn mount(state: Arc<FuseMountState>, mount_point: &Path) -> Result<BackgroundSession> {
    if !fuse_device_available(Path::new("/dev/fuse")) {
        return Err(SesameError::Platform(
            "/dev/fuse is unavailable or is not a character device; load the FUSE kernel module and grant this user access".into(),
        ));
    }
    fuse_adapter::prepare_mount_directory(mount_point)?;
    fuse_adapter::spawn(state, mount_point)
}

fn fuse_device_available(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.file_type().is_char_device())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn rejects_missing_and_regular_files_as_fuse_devices() {
        let directory = TempDir::new().unwrap();
        let missing = directory.path().join("missing");
        let regular = directory.path().join("regular");
        std::fs::write(&regular, b"not a device").unwrap();

        assert!(!fuse_device_available(&missing));
        assert!(!fuse_device_available(&regular));
    }
}
