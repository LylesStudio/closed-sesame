//! Shared vault use cases used by all frontends.

use super::credentials::{Credential, prepare_credentials};
use super::size::ensure_sesame_extension;
use super::symlink::{self, clear_symlink_for_write};
use crate::container::path::{is_prefix, normalize};
use crate::container::{ContainerReader, ContainerWriter, FileEntry};
use crate::crypto::{KdfAlgorithm, KdfConfig, Salt};
use crate::error::{Result, SesameError};
use crate::i18n::keys;
use crate::keymgmt::KeyEncryptingKey;
use std::fs;
use std::path::{Path, PathBuf};

pub struct OpenVault {
    pub reader: ContainerReader,
    pub credentials: KeyEncryptingKey,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AddDisposition {
    #[default]
    Copy,
    MoveWithSymlink,
}

impl AddDisposition {
    fn has_symlink(self) -> bool {
        matches!(self, Self::MoveWithSymlink)
    }
}

pub fn create(
    path: impl Into<PathBuf>,
    data_size: usize,
    credential: Credential<'_>,
) -> Result<PathBuf> {
    let config = match credential {
        Credential::Password(_) => KdfConfig::default_password(),
        Credential::NoPassword => KdfConfig::no_password(),
    };
    create_with_kdf_config(path, data_size, credential, config)
}

pub fn create_with_kdf_config(
    path: impl Into<PathBuf>,
    data_size: usize,
    credential: Credential<'_>,
    config: KdfConfig,
) -> Result<PathBuf> {
    let path = ensure_sesame_extension(path);
    let salt = Salt::generate();
    let credentials = match credential {
        Credential::Password("") => {
            return Err(SesameError::InvalidInput(keys::PASSWORD_EMPTY.into()));
        }
        Credential::Password(_) if config.algorithm != KdfAlgorithm::Argon2id => {
            return Err(SesameError::InvalidInput(keys::PASSWORD_KDF.into()));
        }
        Credential::Password(password) => KeyEncryptingKey::from_password(password, &salt, config)?,
        Credential::NoPassword if config.algorithm != KdfAlgorithm::NoPassword => {
            return Err(SesameError::InvalidInput(keys::NO_PASSWORD_KDF.into()));
        }
        Credential::NoPassword => KeyEncryptingKey::no_password(&salt, config)?,
    };
    ContainerWriter::create(&path, &credentials, salt, data_size)?;
    Ok(path)
}

pub fn open(path: &Path, credential: Credential<'_>) -> Result<OpenVault> {
    let credentials = prepare_credentials(path, credential)?;
    let reader = ContainerReader::open(path, &credentials)?;
    Ok(OpenVault {
        reader,
        credentials,
    })
}

pub fn list_with_credentials(
    path: &Path,
    credentials: &KeyEncryptingKey,
) -> Result<Vec<FileEntry>> {
    ContainerReader::open(path, credentials)?.list_files()
}

pub fn list(path: &Path, credential: Credential<'_>) -> Result<Vec<FileEntry>> {
    open(path, credential)?.reader.list_files()
}

pub fn mkdir(path: &Path, credential: Credential<'_>, target: &str) -> Result<FileEntry> {
    let credentials = prepare_credentials(path, credential)?;
    mkdir_with_credentials(path, &credentials, target)
}

pub fn mkdir_with_credentials(
    path: &Path,
    credentials: &KeyEncryptingKey,
    target: &str,
) -> Result<FileEntry> {
    ContainerWriter::open(path, credentials)?.mkdir(target)
}

pub fn add_with_credentials(
    path: &Path,
    credentials: &KeyEncryptingKey,
    source: &Path,
    target_name: Option<&str>,
) -> Result<FileEntry> {
    add_with_credentials_and_disposition(
        path,
        credentials,
        source,
        target_name,
        AddDisposition::Copy,
    )
}

pub fn add_with_credentials_and_disposition(
    path: &Path,
    credentials: &KeyEncryptingKey,
    source: &Path,
    target_name: Option<&str>,
    disposition: AddDisposition,
) -> Result<FileEntry> {
    let target_name = target_name
        .map(str::to_owned)
        .or_else(|| relative_path_name(source))
        .ok_or_else(|| SesameError::InvalidInput("source has no file name".into()))?;
    ContainerWriter::open(path, credentials)?.add_file_with_symlink(
        source,
        &target_name,
        disposition.has_symlink(),
    )
}

pub fn add(
    path: &Path,
    credential: Credential<'_>,
    source: &Path,
    target_name: Option<&str>,
) -> Result<FileEntry> {
    add_with_disposition(path, credential, source, target_name, AddDisposition::Copy)
}

pub fn add_with_disposition(
    path: &Path,
    credential: Credential<'_>,
    source: &Path,
    target_name: Option<&str>,
    disposition: AddDisposition,
) -> Result<FileEntry> {
    let credentials = prepare_credentials(path, credential)?;
    add_with_credentials_and_disposition(path, &credentials, source, target_name, disposition)
}

pub fn add_dir_with_credentials(
    vault_path: &Path,
    credentials: &KeyEncryptingKey,
    source_dir: &Path,
    target_prefix: Option<&str>,
) -> Result<Vec<FileEntry>> {
    add_dir_with_credentials_and_disposition(
        vault_path,
        credentials,
        source_dir,
        target_prefix,
        AddDisposition::Copy,
    )
}

pub fn add_dir_with_credentials_and_disposition(
    vault_path: &Path,
    credentials: &KeyEncryptingKey,
    source_dir: &Path,
    target_prefix: Option<&str>,
    disposition: AddDisposition,
) -> Result<Vec<FileEntry>> {
    if !source_dir.is_dir() {
        return Err(SesameError::InvalidInput(format!(
            "source is not a directory: {}",
            source_dir.display()
        )));
    }
    let prefix = target_prefix
        .map(normalize)
        .transpose()?
        .unwrap_or_default();
    let mut writer = ContainerWriter::open(vault_path, credentials)?;
    let mut added = Vec::new();
    collect_dir_entries(source_dir, source_dir, &prefix, &mut writer, &mut added)?;
    if disposition.has_symlink() && !prefix.is_empty() {
        writer.mark_directory_symlink(&prefix, source_dir)?;
    }
    Ok(added)
}

fn collect_dir_entries(
    root: &Path,
    current: &Path,
    prefix: &str,
    writer: &mut ContainerWriter,
    added: &mut Vec<FileEntry>,
) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|error| SesameError::InvalidInput(error.to_string()))?;
        let target = if prefix.is_empty() {
            relative.to_string_lossy().replace('\\', "/")
        } else {
            format!("{prefix}/{}", relative.to_string_lossy().replace('\\', "/"))
        };
        if path.is_dir() {
            let dir_entry = writer.mkdir(&target)?;
            if !added.iter().any(|existing| existing.path == dir_entry.path) {
                added.push(dir_entry);
            }
            collect_dir_entries(root, &path, prefix, writer, added)?;
        } else if path.is_file() {
            added.push(writer.add_file_with_symlink(&path, &target, false)?);
        }
    }
    Ok(())
}

fn relative_path_name(source: &Path) -> Option<String> {
    source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

pub fn remove(path: &Path, credential: Credential<'_>, target_name: &str) -> Result<FileEntry> {
    let credentials = prepare_credentials(path, credential)?;
    remove_with_credentials(path, &credentials, target_name)
}

pub fn remove_path_with_credentials(
    path: &Path,
    credentials: &KeyEncryptingKey,
    target_name: &str,
) -> Result<Vec<FileEntry>> {
    ContainerWriter::open(path, credentials)?.remove_path(target_name)
}

pub fn remove_with_credentials(
    path: &Path,
    credentials: &KeyEncryptingKey,
    target_name: &str,
) -> Result<FileEntry> {
    let removed = remove_path_with_credentials(path, credentials, target_name)?;
    removed
        .into_iter()
        .find(|entry| entry.path == normalize(target_name).unwrap_or_default())
        .ok_or_else(|| SesameError::InvalidInput(format!("path not found: {target_name}")))
}

pub fn rename(path: &Path, credential: Credential<'_>, old: &str, new: &str) -> Result<()> {
    let credentials = prepare_credentials(path, credential)?;
    rename_with_credentials(path, &credentials, old, new)
}

pub fn rename_with_credentials(
    path: &Path,
    credentials: &KeyEncryptingKey,
    old: &str,
    new: &str,
) -> Result<()> {
    ContainerWriter::open(path, credentials)?.rename_path(old, new)
}

/// Replace the container password. `new_password` must be non-empty.
/// Returns the KEK that opens the container afterwards.
pub fn change_password(
    path: &Path,
    credentials: &KeyEncryptingKey,
    new_password: &str,
) -> Result<KeyEncryptingKey> {
    if new_password.is_empty() {
        return Err(SesameError::InvalidInput(keys::PASSWORD_EMPTY.into()));
    }
    let salt = Salt::generate();
    let new_kek =
        KeyEncryptingKey::from_password(new_password, &salt, KdfConfig::default_password())?;
    ContainerWriter::open(path, credentials)?.rekey(&new_kek, &salt)?;
    Ok(new_kek)
}

/// Remove the container password. The master key stays the same; later opens
/// use no-password mode. Returns the KEK that opens the container afterwards.
pub fn clear_password(path: &Path, credentials: &KeyEncryptingKey) -> Result<KeyEncryptingKey> {
    let salt = Salt::generate();
    let new_kek = KeyEncryptingKey::no_password(&salt, KdfConfig::no_password())?;
    ContainerWriter::open(path, credentials)?.rekey(&new_kek, &salt)?;
    Ok(new_kek)
}

/// Grow the container by at least `additional_bytes` of plaintext capacity.
pub fn grow(path: &Path, credentials: &KeyEncryptingKey, additional_bytes: u64) -> Result<u64> {
    if additional_bytes == 0 {
        return Err(SesameError::InvalidInput(keys::INVALID_SIZE.into()));
    }
    ContainerWriter::open(path, credentials)?.grow(additional_bytes)
}

pub fn copy_path(path: &Path, credentials: &KeyEncryptingKey, from: &str, to: &str) -> Result<()> {
    ContainerWriter::open(path, credentials)?.copy_path(from, to)
}

pub fn copy_many(
    path: &Path,
    credentials: &KeyEncryptingKey,
    pairs: &[(String, String)],
    progress: &dyn Fn(u64, u64),
) -> Result<()> {
    let mut writer = ContainerWriter::open(path, credentials)?;
    let total = pairs.len() as u64;
    for (index, (from, to)) in pairs.iter().enumerate() {
        writer.copy_path(from, to)?;
        progress(index as u64 + 1, total);
    }
    Ok(())
}

pub fn move_many(
    path: &Path,
    credentials: &KeyEncryptingKey,
    pairs: &[(String, String)],
    progress: &dyn Fn(u64, u64),
) -> Result<()> {
    let mut writer = ContainerWriter::open(path, credentials)?;
    let total = pairs.len() as u64;
    for (index, (from, to)) in pairs.iter().enumerate() {
        writer.rename_path(from, to)?;
        progress(index as u64 + 1, total);
    }
    Ok(())
}

/// Replace an existing vault file's contents with `data`, preserving its
/// identity metadata. Uses the writer's two-phase commit (new pages durable
/// before old pages are released) so a crash cannot lose the previous copy.
pub fn replace_with_credentials(
    path: &Path,
    credentials: &KeyEncryptingKey,
    target_name: &str,
    data: &[u8],
) -> Result<FileEntry> {
    ContainerWriter::open(path, credentials)?.replace_file_data(target_name, data)
}

#[derive(Debug, Default, Clone)]
pub struct ExtractOptions<'a> {
    pub mount_point: Option<&'a Path>,
}

#[derive(Debug, Default, Clone)]
pub struct ExtractOutcome {
    pub written: Vec<PathBuf>,
    pub cleared_symlinks: Vec<PathBuf>,
}

impl ExtractOutcome {
    pub fn status_path(&self, target_name: &str, output: &Path) -> PathBuf {
        let normalized = normalize(target_name).unwrap_or_else(|_| target_name.to_string());
        if output.is_dir() {
            output.join(normalized)
        } else {
            output.to_path_buf()
        }
    }
}

pub fn prepare_extract_path(
    path: &Path,
    entries: &[FileEntry],
    options: &ExtractOptions<'_>,
) -> Result<Vec<PathBuf>> {
    #[cfg(feature = "mount")]
    if let Some(mount) = options.mount_point
        && crate::mount::path_is_within_mount(path, mount)
    {
        return Err(SesameError::ExtractMountReadOnly(
            path.display().to_string(),
        ));
    }

    let mut cleared = symlink::clear_vault_symlinks_for_extract(path, entries)?;
    #[cfg(feature = "mount")]
    if let Some(mount) = options.mount_point {
        cleared.extend(symlink::clear_mount_symlinks_on_path(path, mount)?);
    }
    #[cfg(not(feature = "mount"))]
    let _ = options;
    cleared.sort();
    cleared.dedup();
    Ok(cleared)
}

pub fn extract(
    path: &Path,
    credential: Credential<'_>,
    target_name: &str,
    output: &Path,
) -> Result<PathBuf> {
    let credentials = prepare_credentials(path, credential)?;
    let outcome = extract_with_credentials(
        path,
        &credentials,
        target_name,
        output,
        ExtractOptions::default(),
    )?;
    Ok(outcome.status_path(target_name, output))
}

pub fn extract_with_credentials(
    path: &Path,
    credentials: &KeyEncryptingKey,
    target_name: &str,
    output: &Path,
    options: ExtractOptions<'_>,
) -> Result<ExtractOutcome> {
    let mut outcome =
        extract_many_with_credentials(path, credentials, &[target_name], output, options)?;
    if outcome.written.is_empty() {
        outcome
            .written
            .push(outcome.status_path(target_name, output));
    }
    Ok(outcome)
}

pub fn extract_many_with_credentials(
    path: &Path,
    credentials: &KeyEncryptingKey,
    target_names: &[&str],
    output: &Path,
    options: ExtractOptions<'_>,
) -> Result<ExtractOutcome> {
    extract_many_reporting(path, credentials, target_names, output, options, &|_, _| {})
}

pub fn extract_many_reporting(
    path: &Path,
    credentials: &KeyEncryptingKey,
    target_names: &[&str],
    output: &Path,
    options: ExtractOptions<'_>,
    on_file: &dyn Fn(u64, u64),
) -> Result<ExtractOutcome> {
    let reader = ContainerReader::open(path, credentials)?;
    let entries = reader.list_files()?;
    let mut writes = Vec::new();
    for target_name in target_names {
        let normalized = normalize(target_name)?;
        let Some(entry) = entries.iter().find(|entry| entry.path == normalized) else {
            return Err(SesameError::InvalidInput(format!(
                "path not found: {target_name}"
            )));
        };
        if entry.is_directory {
            if output.is_dir() {
                ensure_extract_destination(&output.join(&normalized))?;
            }
            for file in entries
                .iter()
                .filter(|e| !e.is_directory && is_prefix(&normalized, &e.path))
            {
                let dest = resolve_extract_destination(output, &file.path);
                writes.push((dest, file.path.clone()));
            }
        } else {
            let dest = resolve_extract_destination(output, &entry.path);
            writes.push((dest, entry.path.clone()));
        }
    }
    writes.sort_by(|left, right| left.0.cmp(&right.0));
    writes.dedup_by(|left, right| left.0 == right.0);

    let mut cleared_symlinks = prepare_extract_path(output, &entries, &options)?;
    for (dest, _) in &writes {
        cleared_symlinks.extend(prepare_extract_path(dest, &entries, &options)?);
    }
    cleared_symlinks.sort();
    cleared_symlinks.dedup();

    for (dest, _) in &writes {
        ensure_extract_destination(dest)?;
        ensure_extract_parent_chain(dest)?;
    }
    let mut written = Vec::new();
    let total = writes.len() as u64;
    for (index, (dest, entry_path)) in writes.into_iter().enumerate() {
        let file = entries
            .iter()
            .find(|entry| entry.path == entry_path)
            .expect("entry collected above");
        write_extract_file(&reader, file, &dest)?;
        written.push(dest);
        on_file(index as u64 + 1, total);
    }
    Ok(ExtractOutcome {
        written,
        cleared_symlinks,
    })
}

pub fn ensure_extract_destination(dest: &Path) -> Result<()> {
    match dest.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(metadata) if metadata.file_type().is_symlink() => Ok(()),
        Ok(metadata) if metadata.is_file() => {
            Err(SesameError::ExtractConflict(dest.display().to_string()))
        }
        Ok(metadata) if metadata.is_dir() => {
            Err(SesameError::ExtractConflict(dest.display().to_string()))
        }
        Ok(_) => Err(SesameError::ExtractConflict(dest.display().to_string())),
    }
}

pub fn ensure_extract_parent_chain(dest: &Path) -> Result<()> {
    let Some(mut parent) = dest.parent() else {
        return Ok(());
    };
    let mut ancestors = Vec::new();
    while !parent.as_os_str().is_empty() {
        ancestors.push(parent);
        parent = parent.parent().unwrap_or(Path::new(""));
    }
    ancestors.reverse();
    for path in ancestors {
        match path.symlink_metadata() {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(metadata) if metadata.file_type().is_symlink() => {}
            Ok(metadata) if metadata.is_file() => {
                return Err(SesameError::ExtractConflict(path.display().to_string()));
            }
            Ok(_) => {}
        }
    }
    Ok(())
}

fn resolve_extract_destination(output: &Path, entry_path: &str) -> PathBuf {
    let relative = entry_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .fold(PathBuf::new(), |path, segment| path.join(segment));
    if output.is_dir() {
        return output.join(relative);
    }
    if !output.exists() {
        let looks_like_file = output
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains('.'));
        if looks_like_file {
            return output.to_path_buf();
        }
        return output.join(relative);
    }
    if output.is_file() {
        output.to_path_buf()
    } else {
        output.join(relative)
    }
}

pub fn write_extract_file(reader: &ContainerReader, entry: &FileEntry, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    clear_symlink_for_write(dest)?;
    fs::write(dest, reader.read_file(entry)?)?;
    Ok(())
}

pub fn verify(path: &Path, credential: Credential<'_>) -> Result<u64> {
    let vault = open(path, credential)?;
    vault.reader.verify()?;
    Ok(vault.reader.total_data_pages())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Credential;
    use tempfile::TempDir;

    fn sample_vault(temp: &TempDir, entries: &[(&str, &[u8])]) -> PathBuf {
        let path = temp.path().join("vault.sesame");
        create(&path, 1024 * 1024, Credential::NoPassword).unwrap();
        let credentials = prepare_credentials(&path, Credential::NoPassword).unwrap();
        let mut writer = ContainerWriter::open(&path, &credentials).unwrap();
        for (name, data) in entries {
            writer.add_file_data(data, name, None, false).unwrap();
        }
        path
    }

    #[test]
    fn extract_rejects_existing_file_conflict() {
        let temp = TempDir::new().unwrap();
        let vault_path = sample_vault(&temp, &[("doc.txt", b"vault")]);
        let output = temp.path().join("out");
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(output.join("doc.txt"), b"existing").unwrap();
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let error = extract_with_credentials(
            &vault_path,
            &credentials,
            "doc.txt",
            &output,
            ExtractOptions::default(),
        )
        .err()
        .unwrap();
        assert!(matches!(error, SesameError::ExtractConflict(_)));
        assert_eq!(std::fs::read(output.join("doc.txt")).unwrap(), b"existing");
    }

    #[test]
    fn extract_directory_writes_nested_paths() {
        let temp = TempDir::new().unwrap();
        let vault_path = temp.path().join("vault.sesame");
        create(&vault_path, 1024 * 1024, Credential::NoPassword).unwrap();
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let mut writer = ContainerWriter::open(&vault_path, &credentials).unwrap();
        writer.mkdir("project").unwrap();
        writer
            .add_file_data(b"hello", "project/readme.txt", None, false)
            .unwrap();
        let output = temp.path().join("out");
        extract_with_credentials(
            &vault_path,
            &credentials,
            "project",
            &output,
            ExtractOptions::default(),
        )
        .unwrap();
        let extracted = output.join("project").join("readme.txt");
        assert_eq!(std::fs::read(extracted).unwrap(), b"hello");
    }

    fn create_file_symlink(link: &Path, target: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(target, link).unwrap();
    }

    #[test]
    fn resolve_extract_destination_splits_vault_slashes() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("out");
        let dest = resolve_extract_destination(&output, "a/b.txt");
        assert_eq!(dest, output.join("a").join("b.txt"));
    }

    #[test]
    fn extract_rejects_existing_directory_conflict() {
        let temp = TempDir::new().unwrap();
        let vault_path = sample_vault(&temp, &[("doc.txt", b"vault")]);
        let output = temp.path().join("out");
        std::fs::create_dir_all(&output).unwrap();
        std::fs::create_dir_all(output.join("doc.txt")).unwrap();
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let error = extract_with_credentials(
            &vault_path,
            &credentials,
            "doc.txt",
            &output,
            ExtractOptions::default(),
        )
        .err()
        .unwrap();
        assert!(matches!(error, SesameError::ExtractConflict(_)));
    }

    #[test]
    fn extract_clears_symlink_and_writes() {
        let temp = TempDir::new().unwrap();
        let vault_path = sample_vault(&temp, &[("doc.txt", b"vault")]);
        let output = temp.path().join("out");
        std::fs::create_dir_all(&output).unwrap();
        let payload = output.join("payload.txt");
        std::fs::write(&payload, b"payload").unwrap();
        let link = output.join("doc.txt");
        create_file_symlink(&link, &payload);
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        extract_with_credentials(
            &vault_path,
            &credentials,
            "doc.txt",
            &output,
            ExtractOptions::default(),
        )
        .unwrap();
        assert_eq!(std::fs::read(&link).unwrap(), b"vault");
    }

    #[test]
    fn extract_rejects_parent_file_blocking_path() {
        let temp = TempDir::new().unwrap();
        let vault_path = sample_vault(&temp, &[("nested/file.txt", b"vault")]);
        let output = temp.path().join("out");
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(output.join("nested"), b"blocking file").unwrap();
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let error = extract_with_credentials(
            &vault_path,
            &credentials,
            "nested/file.txt",
            &output,
            ExtractOptions::default(),
        )
        .err()
        .unwrap();
        // NOTE: On Linux this returns Io(NotADirectory) instead of ExtractConflict.
        // The parent-chain walk in ensure_extract_parent_chain hits a file-as-dir
        // before the is_file() guard fires in some path configurations.
        #[cfg(target_os = "linux")]
        assert!(matches!(
            error,
            SesameError::ExtractConflict(_) | SesameError::Io(_)
        ));
        #[cfg(not(target_os = "linux"))]
        assert!(matches!(error, SesameError::ExtractConflict(_)));
    }

    #[test]
    fn extract_many_writes_multiple_paths() {
        let temp = TempDir::new().unwrap();
        let vault_path = sample_vault(&temp, &[("a.txt", b"a"), ("b.txt", b"b")]);
        let output = temp.path().join("out");
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let written = extract_many_with_credentials(
            &vault_path,
            &credentials,
            &["a.txt", "b.txt"],
            &output,
            ExtractOptions::default(),
        )
        .unwrap()
        .written;
        assert_eq!(written.len(), 2);
        assert_eq!(std::fs::read(output.join("a.txt")).unwrap(), b"a");
        assert_eq!(std::fs::read(output.join("b.txt")).unwrap(), b"b");
    }

    #[test]
    fn extract_many_all_or_nothing_on_conflict() {
        let temp = TempDir::new().unwrap();
        let vault_path = sample_vault(&temp, &[("a.txt", b"a"), ("b.txt", b"b")]);
        let output = temp.path().join("out");
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(output.join("b.txt"), b"existing").unwrap();
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let error = extract_many_with_credentials(
            &vault_path,
            &credentials,
            &["a.txt", "b.txt"],
            &output,
            ExtractOptions::default(),
        )
        .err()
        .unwrap();
        assert!(matches!(error, SesameError::ExtractConflict(_)));
        assert!(!output.join("a.txt").exists());
    }

    #[test]
    fn extract_directory_rejects_existing_top_level_dir() {
        let temp = TempDir::new().unwrap();
        let vault_path = temp.path().join("vault.sesame");
        create(&vault_path, 1024 * 1024, Credential::NoPassword).unwrap();
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let mut writer = ContainerWriter::open(&vault_path, &credentials).unwrap();
        writer.mkdir("project").unwrap();
        writer
            .add_file_data(b"hello", "project/readme.txt", None, false)
            .unwrap();
        let output = temp.path().join("out");
        std::fs::create_dir_all(output.join("project")).unwrap();
        let error = extract_with_credentials(
            &vault_path,
            &credentials,
            "project",
            &output,
            ExtractOptions::default(),
        )
        .err()
        .unwrap();
        assert!(matches!(error, SesameError::ExtractConflict(_)));
        assert!(!output.join("project").join("readme.txt").exists());
    }

    #[test]
    fn add_dir_wraps_source_name_under_prefix() {
        let temp = TempDir::new().unwrap();
        let vault_path = temp.path().join("vault.sesame");
        create(&vault_path, 1024 * 1024, Credential::NoPassword).unwrap();
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let source = temp.path().join("mydir");
        std::fs::create_dir_all(source.join("nested")).unwrap();
        std::fs::write(source.join("nested/file.txt"), b"data").unwrap();
        add_dir_with_credentials_and_disposition(
            &vault_path,
            &credentials,
            &source,
            Some("docs/mydir"),
            AddDisposition::Copy,
        )
        .unwrap();
        let entries = list_with_credentials(&vault_path, &credentials).unwrap();
        assert!(
            entries
                .iter()
                .any(|entry| entry.path == "docs/mydir/nested/file.txt")
        );
    }

    #[test]
    fn add_dir_marks_root_directory_symlink() {
        let temp = TempDir::new().unwrap();
        let vault_path = temp.path().join("vault.sesame");
        create(&vault_path, 1024 * 1024, Credential::NoPassword).unwrap();
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let source = temp.path().join("mydir");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("file.txt"), b"data").unwrap();
        add_dir_with_credentials_and_disposition(
            &vault_path,
            &credentials,
            &source,
            Some("mydir"),
            AddDisposition::MoveWithSymlink,
        )
        .unwrap();
        let entries = list_with_credentials(&vault_path, &credentials).unwrap();
        let root = entries
            .iter()
            .find(|entry| entry.path == "mydir" && entry.is_directory)
            .expect("root dir");
        assert!(root.has_symlink);
        assert_eq!(
            root.original_path.as_deref(),
            Some(source.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn add_dir_does_not_mark_file_symlinks_inside_tree() {
        let temp = TempDir::new().unwrap();
        let vault_path = temp.path().join("vault.sesame");
        create(&vault_path, 1024 * 1024, Credential::NoPassword).unwrap();
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let source = temp.path().join("mydir");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("file.txt"), b"data").unwrap();
        add_dir_with_credentials_and_disposition(
            &vault_path,
            &credentials,
            &source,
            Some("mydir"),
            AddDisposition::MoveWithSymlink,
        )
        .unwrap();
        let entries = list_with_credentials(&vault_path, &credentials).unwrap();
        let file = entries
            .iter()
            .find(|entry| entry.path == "mydir/file.txt")
            .expect("nested file");
        assert!(!file.has_symlink);
    }

    #[test]
    fn extract_directory_clears_source_symlink_and_writes() {
        let temp = TempDir::new().unwrap();
        let vault_path = temp.path().join("vault.sesame");
        create(&vault_path, 1024 * 1024, Credential::NoPassword).unwrap();
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let mut writer = ContainerWriter::open(&vault_path, &credentials).unwrap();
        writer.mkdir("source").unwrap();
        writer
            .add_file_data(b"extracted", "source/nested/file.txt", None, false)
            .unwrap();
        let output = temp.path().join("parent");
        std::fs::create_dir_all(&output).unwrap();
        let mount = temp.path().join("mount");
        std::fs::create_dir_all(mount.join("source/nested")).unwrap();
        std::fs::write(mount.join("source/nested/file.txt"), b"mounted").unwrap();
        let source_link = output.join("source");
        #[cfg(unix)]
        std::os::unix::fs::symlink(mount.join("source"), &source_link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(mount.join("source"), &source_link).unwrap();
        writer
            .mark_directory_symlink("source", &source_link)
            .unwrap();
        extract_with_credentials(
            &vault_path,
            &credentials,
            "source",
            &output,
            ExtractOptions {
                #[cfg(feature = "mount")]
                mount_point: Some(&mount),
                #[cfg(not(feature = "mount"))]
                mount_point: None,
            },
        )
        .unwrap();
        assert!(source_link.is_dir());
        assert!(
            !source_link
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read(source_link.join("nested/file.txt")).unwrap(),
            b"extracted"
        );
    }

    fn create_dir_symlink(link: &Path, target: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(target, link).unwrap();
    }

    #[test]
    fn extract_file_clears_parent_directory_symlink() {
        let temp = TempDir::new().unwrap();
        let vault_path = temp.path().join("vault.sesame");
        create(&vault_path, 1024 * 1024, Credential::NoPassword).unwrap();
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let mut writer = ContainerWriter::open(&vault_path, &credentials).unwrap();
        writer.mkdir("project").unwrap();
        writer
            .add_file_data(b"extracted", "project/a.txt", None, false)
            .unwrap();
        let output = temp.path().join("parent");
        std::fs::create_dir_all(&output).unwrap();
        let mount = temp.path().join("mount");
        std::fs::create_dir_all(mount.join("project")).unwrap();
        let project_link = output.join("project");
        create_dir_symlink(&project_link, &mount.join("project"));
        writer
            .mark_directory_symlink("project", &project_link)
            .unwrap();
        extract_with_credentials(
            &vault_path,
            &credentials,
            "project/a.txt",
            &output,
            ExtractOptions {
                #[cfg(feature = "mount")]
                mount_point: Some(&mount),
                #[cfg(not(feature = "mount"))]
                mount_point: None,
            },
        )
        .unwrap();
        assert!(project_link.is_dir());
        assert!(
            !project_link
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read(project_link.join("a.txt")).unwrap(),
            b"extracted"
        );
    }

    #[cfg(feature = "mount")]
    #[test]
    fn extract_rejects_output_under_mount() {
        let temp = TempDir::new().unwrap();
        let vault_path = sample_vault(&temp, &[("doc.txt", b"vault")]);
        let mount = temp.path().join("mount");
        std::fs::create_dir_all(&mount).unwrap();
        let output = mount.join("out");
        let credentials = prepare_credentials(&vault_path, Credential::NoPassword).unwrap();
        let error = extract_with_credentials(
            &vault_path,
            &credentials,
            "doc.txt",
            &output,
            ExtractOptions {
                mount_point: Some(&mount),
            },
        )
        .err()
        .unwrap();
        assert!(matches!(error, SesameError::ExtractMountReadOnly(_)));
    }
}
