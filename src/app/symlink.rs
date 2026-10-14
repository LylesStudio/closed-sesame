//! Symlink lifecycle for files and folders added while a vault is mounted.

use crate::error::{Result, SesameError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct SymlinkRegistry {
    links: HashMap<PathBuf, PathBuf>,
}

impl SymlinkRegistry {
    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    pub fn create(
        &mut self,
        original_path: impl AsRef<Path>,
        mount_point: impl AsRef<Path>,
        container_relative: &str,
        is_directory: bool,
    ) -> Result<()> {
        let original_path = original_path.as_ref().to_path_buf();
        let target = join_mount_path(mount_point.as_ref(), container_relative);
        create_platform_symlink(&original_path, &target, is_directory)?;
        self.links.insert(original_path, target);
        Ok(())
    }

    pub fn move_with_symlink(
        &mut self,
        original_path: impl AsRef<Path>,
        mount_point: impl AsRef<Path>,
        container_relative: &str,
    ) -> Result<()> {
        let original_path = original_path.as_ref();
        if !original_path.is_file() {
            return Err(SesameError::InvalidInput(format!(
                "source is not a regular file: {}",
                original_path.display()
            )));
        }
        let target = join_mount_path(mount_point.as_ref(), container_relative);
        let file_name = original_path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        let staged = original_path.with_file_name(format!(
            ".{file_name}.closed-sesame-link-{}",
            std::process::id()
        ));
        if staged.symlink_metadata().is_ok() {
            return Err(SesameError::InvalidInput(format!(
                "temporary symlink path already exists: {}",
                staged.display()
            )));
        }
        create_platform_symlink(&staged, &target, false)?;
        if let Err(error) = std::fs::remove_file(original_path) {
            let _ = remove_platform_symlink(&staged);
            return Err(error.into());
        }
        if let Err(error) = std::fs::rename(&staged, original_path) {
            let _ = remove_platform_symlink(&staged);
            let _ = std::fs::copy(&target, original_path);
            return Err(error.into());
        }
        self.links
            .insert(original_path.to_path_buf(), target.clone());
        Ok(())
    }

    pub fn move_dir_with_symlink(
        &mut self,
        original_dir: impl AsRef<Path>,
        mount_point: impl AsRef<Path>,
        container_relative: &str,
    ) -> Result<()> {
        let original_dir = original_dir.as_ref();
        if !original_dir.is_dir() {
            return Err(SesameError::InvalidInput(format!(
                "source is not a directory: {}",
                original_dir.display()
            )));
        }
        let target = join_mount_path(mount_point.as_ref(), container_relative);
        let dir_name = original_dir
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        let parent = original_dir
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let staged = parent.join(format!(
            ".{dir_name}.closed-sesame-link-{}",
            std::process::id()
        ));
        if staged.symlink_metadata().is_ok() {
            return Err(SesameError::InvalidInput(format!(
                "temporary symlink path already exists: {}",
                staged.display()
            )));
        }
        create_platform_symlink(&staged, &target, true)?;
        if let Err(error) = std::fs::remove_dir_all(original_dir) {
            let _ = remove_platform_symlink(&staged);
            return Err(error.into());
        }
        if let Err(error) = std::fs::rename(&staged, original_dir) {
            let _ = remove_platform_symlink(&staged);
            return Err(error.into());
        }
        self.links
            .insert(original_dir.to_path_buf(), target.clone());
        Ok(())
    }

    pub fn restore_symlinks(
        &mut self,
        entries: &[crate::FileEntry],
        mount_point: &Path,
    ) -> Vec<(PathBuf, String)> {
        let mut failures = Vec::new();
        for entry in entries {
            if !entry.has_symlink {
                continue;
            }
            let Some(original) = entry.original_path.as_deref() else {
                continue;
            };
            let original_path = PathBuf::from(original);
            match original_path.symlink_metadata() {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    failures.push((original_path, error.to_string()));
                    continue;
                }
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    if let Err(error) = remove_platform_symlink(&original_path) {
                        failures.push((original_path, error.localized()));
                        continue;
                    }
                }
                Ok(_) => continue,
            }
            if let Err(error) =
                self.create(&original_path, mount_point, &entry.path, entry.is_directory)
            {
                failures.push((original_path, error.localized()));
            }
        }
        failures
    }

    pub fn forget(&mut self, original: &Path) {
        self.links.remove(original);
    }

    pub fn remove_all(&mut self) -> Result<()> {
        let originals: Vec<_> = self.links.keys().cloned().collect();
        for original in originals {
            remove_platform_symlink(&original)?;
            self.links.remove(&original);
        }
        Ok(())
    }
}

pub fn join_mount_path(mount_point: &Path, container_relative: &str) -> PathBuf {
    let relative = container_relative.replace('/', std::path::MAIN_SEPARATOR_STR);
    let mount = mount_point.as_os_str().to_string_lossy();
    let bytes = mount.as_bytes();
    if cfg!(windows)
        && ((bytes.len() == 2 && bytes[1] == b':')
            || (bytes.len() == 3 && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/')))
    {
        return PathBuf::from(format!("{}\\{relative}", &mount[..2]));
    }
    mount_point.join(relative)
}

/// Remove a symlink at `path` if present.
pub fn remove_symlink_at(path: &Path) -> Result<()> {
    remove_platform_symlink(path)
}

/// Remove a symlink at `path` so a regular file can be written there.
pub fn clear_symlink_for_write(path: &Path) -> Result<bool> {
    match path.symlink_metadata() {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            remove_platform_symlink(path)?;
            Ok(true)
        }
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[cfg_attr(not(feature = "mount"), allow(dead_code))]
fn path_prefixes(path: &Path) -> Vec<PathBuf> {
    let mut acc = PathBuf::new();
    let mut prefixes = Vec::new();
    for component in path.components() {
        acc.push(component);
        prefixes.push(acc.clone());
    }
    prefixes
}

/// Walk path prefixes shallow-to-deep and remove symlinks that point into `mount_point`.
#[cfg(feature = "mount")]
pub fn clear_mount_symlinks_on_path(path: &Path, mount_point: &Path) -> Result<Vec<PathBuf>> {
    let mut cleared = Vec::new();
    for prefix in path_prefixes(path) {
        let Ok(metadata) = prefix.symlink_metadata() else {
            continue;
        };
        if !metadata.file_type().is_symlink() {
            continue;
        }
        let Ok(target) = std::fs::read_link(&prefix) else {
            continue;
        };
        if crate::mount::path_is_within_mount(&target, mount_point)
            && clear_symlink_for_write(&prefix)?
        {
            cleared.push(prefix);
        }
    }
    Ok(cleared)
}

/// Remove vault-managed symlinks on `dest` using container metadata.
pub fn clear_vault_symlinks_for_extract(
    dest: &Path,
    entries: &[crate::FileEntry],
) -> Result<Vec<PathBuf>> {
    let mut cleared = Vec::new();
    let mut candidates: Vec<PathBuf> = entries
        .iter()
        .filter(|entry| entry.has_symlink)
        .filter_map(|entry| entry.original_path.as_deref().map(PathBuf::from))
        .collect();
    candidates.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    candidates.dedup();
    for original in candidates {
        if dest.starts_with(&original) && clear_symlink_for_write(&original)? {
            cleared.push(original);
        }
    }
    Ok(cleared)
}

fn create_platform_symlink(original: &Path, target: &Path, is_directory: bool) -> Result<()> {
    if original.exists() {
        return Err(SesameError::InvalidInput(format!(
            "cannot create symlink; path already exists: {}",
            original.display()
        )));
    }
    if let Some(parent) = original.parent() {
        std::fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        let _ = is_directory;
        std::os::unix::fs::symlink(target, original)?;
    }
    #[cfg(windows)]
    {
        let result = if is_directory {
            std::os::windows::fs::symlink_dir(target, original)
        } else {
            std::os::windows::fs::symlink_file(target, original)
        };
        result.map_err(|error| {
            SesameError::Platform(format!(
                "failed to create symlink at {} -> {}: {error}. Enable Developer Mode or run as administrator.",
                original.display(),
                target.display()
            ))
        })?;
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (original, target, is_directory);
        return Err(SesameError::Platform(
            "symlinks are unsupported on this platform".into(),
        ));
    }
    Ok(())
}

fn remove_platform_symlink(original: &Path) -> Result<()> {
    if !original.exists() && original.symlink_metadata().is_err() {
        return Ok(());
    }
    let metadata = original.symlink_metadata()?;
    if metadata.file_type().is_symlink() {
        #[cfg(windows)]
        {
            // Directory symlinks report is_dir() == false on symlink_metadata(); remove_file
            // fails with access denied. Try remove_dir first, then remove_file for file links.
            if std::fs::remove_dir(original).is_err() {
                std::fs::remove_file(original)?;
            }
        }
        #[cfg(not(windows))]
        {
            if metadata.is_dir() {
                std::fs::remove_dir(original)?;
            } else {
                std::fs::remove_file(original)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn registry_tracks_and_clears_links() {
        let temp = TempDir::new().unwrap();
        let mount = temp.path().join("mount");
        let payload = mount.join("secret.txt");
        std::fs::create_dir_all(&mount).unwrap();
        std::fs::write(&payload, b"data").unwrap();
        let original = temp.path().join("link.txt");
        let mut registry = SymlinkRegistry::default();
        registry
            .create(&original, &mount, "secret.txt", false)
            .expect("symlink");
        assert!(original.exists());
        registry.remove_all().expect("cleanup");
        assert!(!original.exists());
    }

    #[test]
    fn remove_all_clears_directory_symlink() {
        if !symlink_permissions_available() {
            eprintln!("SKIP remove_all_clears_directory_symlink: symlink permission denied");
            return;
        }
        let temp = TempDir::new().unwrap();
        let mount = temp.path().join("mount");
        std::fs::create_dir_all(mount.join("project")).unwrap();
        let original = temp.path().join("project");
        std::fs::create_dir_all(original.join("nested")).unwrap();
        let mut registry = SymlinkRegistry::default();
        registry
            .move_dir_with_symlink(&original, &mount, "project")
            .expect("move dir with symlink");
        assert!(
            original
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        registry.remove_all().expect("cleanup");
        assert!(original.symlink_metadata().is_err());
    }

    #[test]
    fn move_replaces_source_with_symlink() {
        let temp = TempDir::new().unwrap();
        let mount = temp.path().join("mount");
        let payload = mount.join("secret.txt");
        std::fs::create_dir_all(&mount).unwrap();
        std::fs::write(&payload, b"data").unwrap();
        let original = temp.path().join("secret.txt");
        std::fs::write(&original, b"data").unwrap();

        let mut registry = SymlinkRegistry::default();
        registry
            .move_with_symlink(&original, &mount, "secret.txt")
            .expect("move with symlink");

        assert!(
            original
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(std::fs::read(&original).unwrap(), b"data");
    }

    #[test]
    fn remove_symlink_at_deletes_link_only() {
        let temp = TempDir::new().unwrap();
        let mount = temp.path().join("mount");
        let payload = mount.join("secret.txt");
        std::fs::create_dir_all(&mount).unwrap();
        std::fs::write(&payload, b"mounted").unwrap();
        let original = temp.path().join("secret.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&payload, &original).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&payload, &original).unwrap();

        remove_symlink_at(&original).unwrap();
        assert!(!original.exists());
        assert!(payload.exists());
    }

    #[test]
    fn clear_symlink_for_write_allows_regular_file() {
        let temp = TempDir::new().unwrap();
        let mount = temp.path().join("mount");
        let payload = mount.join("secret.txt");
        std::fs::create_dir_all(&mount).unwrap();
        std::fs::write(&payload, b"mounted").unwrap();
        let original = temp.path().join("secret.txt");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&payload, &original).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(&payload, &original).unwrap();

        assert!(clear_symlink_for_write(&original).unwrap());
        std::fs::write(&original, b"restored").unwrap();
        assert_eq!(std::fs::read(&original).unwrap(), b"restored");
    }

    #[test]
    fn move_dir_replaces_source_with_symlink() {
        if !symlink_permissions_available() {
            eprintln!("SKIP move_dir_replaces_source_with_symlink: symlink permission denied");
            return;
        }
        let temp = TempDir::new().unwrap();
        let mount = temp.path().join("mount");
        std::fs::create_dir_all(mount.join("project/nested")).unwrap();
        std::fs::write(mount.join("project/nested/file.txt"), b"mounted").unwrap();
        let original = temp.path().join("project");
        std::fs::create_dir_all(original.join("nested")).unwrap();
        std::fs::write(original.join("nested/file.txt"), b"local").unwrap();

        let mut registry = SymlinkRegistry::default();
        registry
            .move_dir_with_symlink(&original, &mount, "project")
            .expect("move dir with symlink");

        assert!(
            original
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(original.join("nested/file.txt")).unwrap(),
            "mounted"
        );
    }

    #[test]
    fn restore_creates_directory_symlink() {
        if !symlink_permissions_available() {
            eprintln!("SKIP restore_creates_directory_symlink: symlink permission denied");
            return;
        }
        let temp = TempDir::new().unwrap();
        let mount = temp.path().join("mount");
        std::fs::create_dir_all(mount.join("docs")).unwrap();
        std::fs::write(mount.join("docs/readme.txt"), b"mounted").unwrap();
        let original = temp.path().join("docs");
        let entries = vec![crate::FileEntry {
            path: "docs".into(),
            original_path: Some(original.to_string_lossy().into_owned()),
            size: 0,
            start_page: 0,
            page_count: 0,
            crc32: 0,
            created_at: 0,
            modified_at: 0,
            has_symlink: true,
            is_directory: true,
        }];
        let mut registry = SymlinkRegistry::default();
        let failures = registry.restore_symlinks(&entries, &mount);
        assert!(failures.is_empty());
        assert!(
            original
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    fn symlink_permissions_available() -> bool {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("target.txt");
        let link = temp.path().join("link.txt");
        std::fs::write(&target, b"x").unwrap();
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(&target, &link).is_ok();
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_file(&target, &link).is_ok();
        #[cfg(not(any(unix, windows)))]
        let result = false;
        let _ = std::fs::remove_file(link);
        result
    }
}
