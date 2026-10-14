//! GUI-independent vault actions and unlocked-session state.

use crate::app::{
    Credential,
    symlink::SymlinkRegistry,
    vault::{self, AddDisposition},
};
use crate::container::path::join;
use crate::error::{Result, SesameError};
use crate::i18n::keys;
use crate::keymgmt::KeyEncryptingKey;
use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};

pub(crate) struct Session {
    path: PathBuf,
    credentials: KeyEncryptingKey,
    pub(crate) symlinks: SymlinkRegistry,
    pub(crate) create_symlinks: bool,
    /// Held for the whole unlock so another process cannot open the container.
    container_lock: Option<crate::container::VaultLock>,
}

impl Session {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn credentials(&self) -> &KeyEncryptingKey {
        &self.credentials
    }

    pub(crate) fn set_credentials(&mut self, credentials: KeyEncryptingKey) {
        self.credentials = credentials;
    }

    /// Drop the process lock so an operation that replaces the container file
    /// (defrag) can rename it. Call [`resume_lock`] when that operation ends.
    pub(crate) fn suspend_lock(&mut self) {
        self.container_lock.take();
    }

    pub(crate) fn resume_lock(&mut self) -> Result<()> {
        if self.container_lock.is_none() {
            self.container_lock = Some(crate::container::VaultLock::try_acquire(&self.path)?);
        }
        Ok(())
    }

    /// A lock-free session for a background worker. The unlocked GUI session
    /// already holds the process lock.
    pub(crate) fn for_worker(
        path: PathBuf,
        credentials: KeyEncryptingKey,
        create_symlinks: bool,
    ) -> Self {
        Self {
            path,
            credentials,
            symlinks: SymlinkRegistry::default(),
            create_symlinks,
            container_lock: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileEntryInfo {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) size: u64,
    pub(crate) is_dir: bool,
    pub(crate) modified_at: u64,
}

pub(crate) struct OpenResult {
    pub(crate) session: Session,
    pub(crate) entries: Vec<FileEntryInfo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AddOutcome {
    Added { links: Vec<(PathBuf, String)> },
    Rejected { path: PathBuf, reason: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BatchAddResult {
    pub(crate) outcomes: Vec<AddOutcome>,
    pub(crate) entries: Vec<FileEntryInfo>,
    pub(crate) refresh_error: Option<String>,
}

impl BatchAddResult {
    pub(crate) fn added_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| matches!(outcome, AddOutcome::Added { .. }))
            .count()
    }

    pub(crate) fn rejected_count(&self) -> usize {
        self.outcomes.len() - self.added_count()
    }
}

fn credential<'a>(password: &'a str, no_password: bool) -> Credential<'a> {
    if no_password {
        Credential::NoPassword
    } else {
        Credential::Password(password)
    }
}

fn validate_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(SesameError::InvalidInput(keys::SESAME_PATH_REQUIRED.into()));
    }
    Ok(())
}

fn entries_from_vault(entries: Vec<crate::FileEntry>) -> Vec<FileEntryInfo> {
    entries
        .into_iter()
        .map(|entry| FileEntryInfo {
            name: entry
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&entry.path)
                .to_owned(),
            path: entry.path,
            size: entry.size,
            is_dir: entry.is_directory,
            modified_at: entry.modified_at,
        })
        .collect()
}

fn new_session(path: PathBuf, credentials: KeyEncryptingKey) -> Result<Session> {
    let container_lock = Some(crate::container::VaultLock::try_acquire(&path)?);
    Ok(Session {
        path,
        credentials,
        symlinks: SymlinkRegistry::default(),
        create_symlinks: false,
        container_lock,
    })
}

pub(crate) fn open(path: &Path, password: &str, no_password: bool) -> Result<OpenResult> {
    validate_path(path)?;
    let opened = vault::open(path, credential(password, no_password))?;
    let entries = entries_from_vault(opened.reader.list_files()?);
    Ok(OpenResult {
        session: new_session(path.to_path_buf(), opened.credentials)?,
        entries,
    })
}

pub(crate) fn create(
    path: &Path,
    data_size: usize,
    password: &str,
    no_password: bool,
) -> Result<OpenResult> {
    validate_path(path)?;
    let path = vault::create(
        path.to_path_buf(),
        data_size,
        credential(password, no_password),
    )?;
    open(&path, password, no_password)
}

pub(crate) fn plan_transfers(
    entries: &[FileEntryInfo],
    sources: &[String],
    dest_dir: &str,
    copy: bool,
) -> Result<Vec<(String, String)>> {
    let mut selected = BTreeSet::new();
    for source in sources {
        selected.insert(source.clone());
    }
    let sources = super::selection::dedupe_delete_paths(&selected);
    let dest_dir = if dest_dir.trim().is_empty() {
        String::new()
    } else {
        crate::container::path::normalize(dest_dir.trim())?
    };
    if !dest_dir.is_empty()
        && entries
            .iter()
            .any(|entry| entry.path == dest_dir && !entry.is_dir)
    {
        return Err(SesameError::InvalidInput(keys::DESTINATION_EXISTS.into()));
    }
    let mut used: BTreeSet<String> = entries.iter().map(|entry| entry.path.clone()).collect();
    let mut pairs = Vec::new();
    for source in sources {
        let leaf = source.rsplit('/').next().unwrap_or(&source);
        let mut dest = if dest_dir.is_empty() {
            leaf.to_string()
        } else {
            join(&dest_dir, leaf)?
        };
        if copy {
            let mut n = 1u32;
            while used.contains(&dest) {
                let renamed = disambiguate(leaf, n);
                dest = if dest_dir.is_empty() {
                    renamed
                } else {
                    join(&dest_dir, &renamed)?
                };
                n += 1;
                if n > 1000 {
                    return Err(SesameError::InvalidInput(keys::DESTINATION_EXISTS.into()));
                }
            }
        } else if dest != source && used.contains(&dest) {
            return Err(SesameError::InvalidInput(keys::DESTINATION_EXISTS.into()));
        }
        if dest != source {
            used.insert(dest.clone());
            pairs.push((source, dest));
        }
    }
    Ok(pairs)
}

fn disambiguate(leaf: &str, n: u32) -> String {
    let (stem, ext) = match leaf.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (leaf, String::new()),
    };
    if n == 1 {
        format!("{stem} copy{ext}")
    } else {
        format!("{stem} copy {n}{ext}")
    }
}

pub(crate) fn refresh(session: &Session) -> Result<Vec<FileEntryInfo>> {
    vault::list_with_credentials(&session.path, &session.credentials).map(entries_from_vault)
}

/// Page-level capacity of the container data area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Capacity {
    pub(crate) used_bytes: u64,
    pub(crate) total_bytes: u64,
}

impl Capacity {
    pub(crate) fn ratio(self) -> f32 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.used_bytes as f64 / self.total_bytes as f64).clamp(0.0, 1.0) as f32
        }
    }
}

/// Allocated vs. total data pages, expressed in plaintext bytes.
pub(crate) fn capacity(session: &Session) -> Result<Capacity> {
    let reader = crate::container::ContainerReader::open(&session.path, &session.credentials)?;
    let page = crate::container::format::PAGE_PLAINTEXT as u64;
    Ok(Capacity {
        used_bytes: reader.bitmap().allocated_pages() * page,
        total_bytes: reader.total_data_pages() * page,
    })
}

fn reject(path: PathBuf, reason: impl Into<String>) -> AddOutcome {
    AddOutcome::Rejected {
        path,
        reason: reason.into(),
    }
}

pub(crate) fn add_many(
    session: &Session,
    paths: impl IntoIterator<Item = PathBuf>,
    current_dir: &str,
) -> BatchAddResult {
    add_many_with_progress(session, paths, current_dir, &|_, _, _| {})
}

pub(crate) fn add_many_with_progress(
    session: &Session,
    paths: impl IntoIterator<Item = PathBuf>,
    current_dir: &str,
    on_item: &dyn Fn(u64, u64, &str),
) -> BatchAddResult {
    let mut result = BatchAddResult::default();
    let disposition = if session.create_symlinks {
        AddDisposition::MoveWithSymlink
    } else {
        AddDisposition::Copy
    };

    let paths: Vec<PathBuf> = paths.into_iter().collect();
    let total = paths.len() as u64;
    for (index, path) in paths.into_iter().enumerate() {
        on_item(index as u64 + 1, total, &path.display().to_string());
        let outcome = if path.as_os_str().is_empty() {
            reject(path, keys::EMPTY_PATH)
        } else {
            match path.metadata() {
                Err(error) => reject(path, format!("{}:{}", keys::CANNOT_INSPECT_PATH, error)),
                Ok(metadata) if metadata.is_dir() => {
                    let dir_name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned());
                    let target_prefix = match (current_dir.is_empty(), dir_name.as_deref()) {
                        (true, Some(name)) => Some(name.to_string()),
                        (false, Some(name)) => join(current_dir, name).ok(),
                        (true, None) => None,
                        (false, None) => Some(current_dir.to_string()),
                    };
                    match vault::add_dir_with_credentials_and_disposition(
                        &session.path,
                        &session.credentials,
                        &path,
                        target_prefix.as_deref(),
                        disposition,
                    ) {
                        Ok(entries) => {
                            let links = if session.create_symlinks {
                                target_prefix
                                    .map(|container_dir| vec![(path.clone(), container_dir)])
                                    .unwrap_or_default()
                            } else {
                                entries
                                    .into_iter()
                                    .filter(|entry| !entry.is_directory)
                                    .filter_map(|entry| {
                                        entry
                                            .original_path
                                            .map(|original| (PathBuf::from(original), entry.path))
                                    })
                                    .collect()
                            };
                            AddOutcome::Added { links }
                        }
                        Err(error) => reject(path, error.localized()),
                    }
                }
                Ok(_) => match File::open(&path) {
                    Err(error) => reject(path, format!("{}:{}", keys::FILE_NOT_READABLE, error)),
                    Ok(_) => {
                        let target = path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .and_then(|name| {
                                if current_dir.is_empty() {
                                    Some(name)
                                } else {
                                    join(current_dir, &name).ok()
                                }
                            });
                        match target {
                            None => reject(path, keys::INVALID_TARGET_PATH),
                            Some(target) => match vault::add_with_credentials_and_disposition(
                                &session.path,
                                &session.credentials,
                                &path,
                                Some(&target),
                                disposition,
                            ) {
                                Ok(entry) => AddOutcome::Added {
                                    links: vec![(path, entry.path)],
                                },
                                Err(error) => reject(path, error.localized()),
                            },
                        }
                    }
                },
            }
        };
        result.outcomes.push(outcome);
    }

    match refresh(session) {
        Ok(entries) => result.entries = entries,
        Err(error) => result.refresh_error = Some(error.localized()),
    }
    result
}

pub(crate) fn mkdir(session: &Session, name: &str, current_dir: &str) -> Result<()> {
    let target = if current_dir.is_empty() {
        name.to_owned()
    } else {
        join(current_dir, name)?
    };
    vault::mkdir_with_credentials(&session.path, &session.credentials, &target)?;
    Ok(())
}

pub(crate) fn cleanup_symlinks_for_removed(session: &mut Session, removed: &[crate::FileEntry]) {
    use crate::app::symlink::remove_symlink_at;
    for entry in removed {
        if !entry.has_symlink {
            continue;
        }
        let Some(original) = entry.original_path.as_deref() else {
            continue;
        };
        let original = PathBuf::from(original);
        session.symlinks.forget(&original);
        let _ = remove_symlink_at(&original);
    }
}

pub(crate) fn rename(session: &Session, old: &str, new: &str) -> Result<()> {
    vault::rename_with_credentials(&session.path, &session.credentials, old, new)
}

/// Decrypt a vault file into memory (for the cache session), returning the
/// bytes and the entry metadata used as the conflict anchor.
pub(crate) fn read_vault_file(
    session: &Session,
    target: &str,
) -> Result<(Vec<u8>, crate::FileEntry)> {
    let reader = crate::container::ContainerReader::open(&session.path, &session.credentials)?;
    let entries = reader.list_files()?;
    let normalized = crate::container::path::normalize(target)?;
    let entry = entries
        .iter()
        .find(|entry| entry.path == normalized)
        .ok_or_else(|| SesameError::InvalidInput(format!("path not found: {target}")))?;
    if entry.is_directory {
        return Err(SesameError::InvalidInput(format!(
            "cannot open directory as a file: {target}"
        )));
    }
    let data = reader.read_file(entry)?;
    Ok((data, entry.clone()))
}

/// Live `modified_at` of a vault entry, read straight from the container.
///
/// Used for cache write-back conflict checks: the GUI's cached entry list
/// goes stale when edits happen through a mount, so the anchor must come
/// from the vault itself. Returns `None` when the entry no longer exists.
pub(crate) fn vault_modified_at(session: &Session, target: &str) -> Result<Option<u64>> {
    let entries = vault::list_with_credentials(&session.path, &session.credentials)?;
    let normalized = crate::container::path::normalize(target)?;
    Ok(entries
        .iter()
        .find(|entry| entry.path == normalized)
        .map(|entry| entry.modified_at))
}

/// Write edited cache content back into the vault with a two-phase commit.
pub(crate) fn replace_vault_file(
    session: &Session,
    target: &str,
    data: &[u8],
) -> Result<crate::FileEntry> {
    vault::replace_with_credentials(&session.path, &session.credentials, target, data)
}

/// Add edited cache content as a brand-new vault entry (conflict copy).
pub(crate) fn add_vault_file(
    session: &Session,
    target: &str,
    data: &[u8],
) -> Result<crate::FileEntry> {
    crate::container::ContainerWriter::open(&session.path, &session.credentials)?
        .add_file_data(data, target, None, false)
}

#[cfg(feature = "mount")]
pub(crate) fn mount(
    kek: &crate::keymgmt::KeyEncryptingKey,
    container: &Path,
    mount_point: &Path,
) -> Result<crate::mount::MountedVault> {
    crate::mount::check_prerequisites()?;
    #[cfg(target_os = "macos")]
    {
        // macOS mounts via the loopback WebDAV bridge: Finder supports
        // WebDAV natively, no macFUSE/FSKit runtime required.
        let _ = mount_point;
        crate::mount::mount_webdav_with_kek(kek, container)
    }
    #[cfg(not(target_os = "macos"))]
    {
        crate::mount::mount_vault_with_kek(kek, container, mount_point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_vault_path_is_rejected() {
        let error = open(Path::new(""), "", true).err().unwrap();
        assert!(matches!(error, SesameError::InvalidInput(_)));
    }

    #[test]
    fn creates_and_opens_no_password_vault() {
        let temp = tempfile::tempdir().unwrap();
        let created = create(&temp.path().join("test-vault"), 1024 * 1024, "", true).unwrap();

        assert!(created.session.path.ends_with("test-vault.sesame"));
        assert!(created.entries.is_empty());
        assert!(created.session.path.exists());

        let reopened = open(&created.session.path, "", true).unwrap();
        assert!(reopened.entries.is_empty());
    }

    #[test]
    fn add_folder_at_vault_root_succeeds() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir_all(source.join("nested")).unwrap();
        std::fs::write(source.join("nested/file.txt"), b"hello").unwrap();

        let created = create(&temp.path().join("vault"), 1024 * 1024, "", true).unwrap();
        let result = add_many(&created.session, vec![source], "");
        assert_eq!(result.added_count(), 1, "{:?}", result.outcomes);
        assert!(
            result
                .entries
                .iter()
                .any(|entry| entry.path == "source/nested/file.txt")
        );
    }

    #[test]
    fn add_folder_under_current_dir_wraps_name() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("pkg");
        std::fs::create_dir_all(source.join("inner")).unwrap();
        std::fs::write(source.join("inner/file.txt"), b"data").unwrap();

        let created = create(&temp.path().join("vault"), 1024 * 1024, "", true).unwrap();
        let result = add_many(&created.session, vec![source], "docs");
        assert_eq!(result.added_count(), 1);
        assert!(
            result
                .entries
                .iter()
                .any(|entry| entry.path == "docs/pkg/inner/file.txt")
        );
    }

    #[test]
    fn add_folder_with_symlink_links_only_root() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        std::fs::create_dir_all(source.join("nested")).unwrap();
        std::fs::write(source.join("nested/file.txt"), b"data").unwrap();

        let created = create(&temp.path().join("vault"), 1024 * 1024, "", true).unwrap();
        let mut session = created.session;
        session.create_symlinks = true;
        let result = add_many(&session, vec![source.clone()], "");
        assert_eq!(result.added_count(), 1);
        let links = match &result.outcomes[0] {
            AddOutcome::Added { links } => links,
            other => panic!("expected added outcome: {other:?}"),
        };
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, source);
        assert_eq!(links[0].1, "source");
    }
}
