//! Best-effort recovery workflows.

use super::credentials::Credential;
use super::vault;
use crate::app::symlink::clear_symlink_for_write;
use crate::container::FileEntry;
use crate::container::verify_crc32;
use crate::error::{Result, SesameError};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum RecoveryOutcome {
    Extracted(PathBuf),
    SkippedCorrupt,
    Failed(String),
}

#[derive(Debug)]
pub struct RecoveryItem {
    pub path: String,
    pub outcome: RecoveryOutcome,
}

#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub items: Vec<RecoveryItem>,
}

impl RecoveryReport {
    pub fn extracted(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item.outcome, RecoveryOutcome::Extracted(_)))
            .count()
    }

    pub fn failed(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item.outcome, RecoveryOutcome::Failed(_)))
            .count()
    }

    pub fn skipped(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item.outcome, RecoveryOutcome::SkippedCorrupt))
            .count()
    }
}

pub fn verify(container: &Path, credential: Credential<'_>) -> Result<u64> {
    vault::verify(container, credential)
}

pub fn list(container: &Path, credential: Credential<'_>) -> Result<Vec<FileEntry>> {
    vault::list(container, credential)
}

pub fn extract_all(
    container: &Path,
    credential: Credential<'_>,
    output: &Path,
    skip_corrupted: bool,
) -> Result<RecoveryReport> {
    extract_all_with_progress(container, credential, output, skip_corrupted, &|_, _| {})
}

pub fn extract_all_with_progress(
    container: &Path,
    credential: Credential<'_>,
    output: &Path,
    skip_corrupted: bool,
    on_file: &dyn Fn(u64, u64),
) -> Result<RecoveryReport> {
    let opened = vault::open(container, credential)?;
    extract_reader(&opened.reader, output, skip_corrupted, on_file)
}

pub fn extract_reader(
    reader: &crate::container::ContainerReader,
    output: &Path,
    skip_corrupted: bool,
    on_file: &dyn Fn(u64, u64),
) -> Result<RecoveryReport> {
    std::fs::create_dir_all(output)?;
    let entries = reader.list_files()?;
    let files: Vec<_> = entries.iter().filter(|entry| !entry.is_directory).collect();
    let total = files.len() as u64;
    let mut report = RecoveryReport::default();
    for (index, entry) in files.iter().enumerate() {
        if entry.is_directory {
            continue;
        }
        let outcome = match reader.read_file(entry) {
            Ok(data) if !verify_crc32(&data, entry.crc32) && skip_corrupted => {
                RecoveryOutcome::SkippedCorrupt
            }
            Ok(data) => {
                let destination = output.join(
                    entry
                        .path
                        .split('/')
                        .filter(|segment| !segment.is_empty())
                        .fold(PathBuf::new(), |path, segment| path.join(segment)),
                );
                let write_result = (|| -> Result<()> {
                    vault::prepare_extract_path(
                        &destination,
                        &entries,
                        &vault::ExtractOptions::default(),
                    )?;
                    vault::ensure_extract_destination(&destination)?;
                    vault::ensure_extract_parent_chain(&destination)?;
                    if let Some(parent) = destination.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    clear_symlink_for_write(&destination)?;
                    std::fs::write(&destination, data)?;
                    Ok(())
                })();
                match write_result {
                    Ok(()) => RecoveryOutcome::Extracted(destination),
                    Err(SesameError::ExtractConflict(path)) => RecoveryOutcome::Failed(path),
                    Err(SesameError::ExtractMountReadOnly(path)) => RecoveryOutcome::Failed(path),
                    Err(error) => RecoveryOutcome::Failed(error.localized()),
                }
            }
            Err(error) => RecoveryOutcome::Failed(error.to_string()),
        };
        report.items.push(RecoveryItem {
            path: entry.path.clone(),
            outcome,
        });
        on_file(index as u64 + 1, total);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Credential;
    use tempfile::TempDir;

    fn sample_vault(temp: &TempDir, files: &[(&str, &[u8])]) -> PathBuf {
        let path = temp.path().join("vault.sesame");
        vault::create(&path, 1024 * 1024, Credential::NoPassword).unwrap();
        for (name, contents) in files {
            let source = temp.path().join(format!("src-{name}"));
            std::fs::write(&source, contents).unwrap();
            vault::add(&path, Credential::NoPassword, &source, Some(name)).unwrap();
        }
        path
    }

    fn create_file_symlink(link: &Path, target: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(target, link).unwrap();
    }

    #[test]
    fn extract_all_fails_on_existing_file() {
        let temp = TempDir::new().unwrap();
        let vault_path = sample_vault(&temp, &[("doc.txt", b"vault")]);
        let output = temp.path().join("out");
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(output.join("doc.txt"), b"existing").unwrap();
        let report = extract_all(&vault_path, Credential::NoPassword, &output, false).unwrap();
        assert_eq!(report.failed(), 1);
        assert_eq!(report.extracted(), 0);
        assert_eq!(std::fs::read(output.join("doc.txt")).unwrap(), b"existing");
    }

    #[test]
    fn extract_all_clears_symlink_target() {
        let temp = TempDir::new().unwrap();
        let vault_path = sample_vault(&temp, &[("doc.txt", b"vault")]);
        let output = temp.path().join("out");
        std::fs::create_dir_all(&output).unwrap();
        let payload = output.join("payload.txt");
        std::fs::write(&payload, b"payload").unwrap();
        let link = output.join("doc.txt");
        create_file_symlink(&link, &payload);
        let report = extract_all(&vault_path, Credential::NoPassword, &output, false).unwrap();
        assert_eq!(report.extracted(), 1);
        assert_eq!(std::fs::read(&link).unwrap(), b"vault");
    }
}
