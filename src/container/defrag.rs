//! Defragmentation — compact files, reclaim space, verify integrity.
//!
//! # Safety guarantees
//!
//! 1. **Source untouched**: the original container is opened read-only.
//! 2. **Read-verify**: every file read from source has its CRC32 checked against stored value.
//! 3. **Write-verify**: after writing all files to dest, re-open dest and verify every CRC32.
//! 4. **Atomic swap**: only after both verifications pass does `.sesame.DEFRAG` atomically
//!    rename to `.sesame`.
//! 5. **Crash-safe**: if the process dies mid-defrag, `.sesame.DEFRAG` is orphaned garbage.
//!    Next container open detects and removes it.
//!
//! # Algorithm
//!
//! ```text
//!   1. Open source (read-only)
//!   2. Read all files, verify CRC32 on read
//!   3. Compute total size, create dest container
//!   4. Write all files to dest via add_file_data
//!   5. Close dest, re-open, verify CRC32 of every file on disk
//!   6. fsync(dest), rename(dest → source)
//! ```

use crate::container::checksum::crc32;
use crate::container::{ContainerReader, ContainerWriter, FileEntry};
use crate::crypto::Salt;
use crate::error::{Result, SesameError};
use crate::keymgmt::KeyEncryptingKey;
use std::fs;
use std::path::Path;

/// Statistics from a defragmentation run.
#[derive(Debug, Clone)]
pub struct DefragStats {
    pub files_moved: usize,
    pub pages_before: u64,
    pub pages_after: u64,
    pub space_reclaimed_pages: u64,
    pub fragmentation_before: f64,
    pub fragmentation_after: f64,
    pub duration_secs: f64,
}

/// Defragment a container.
///
/// Copies all files to a new compact container, verifies integrity
/// at every stage, then atomically replaces the original.
///
/// If `force` is false and fragmentation is below 15%, returns early
/// without doing any work.
pub fn defragment(sbox_path: &Path, kek: &KeyEncryptingKey, force: bool) -> Result<DefragStats> {
    defragment_with_progress(sbox_path, kek, force, &|_, _| {})
}

pub fn defragment_with_progress(
    sbox_path: &Path,
    kek: &KeyEncryptingKey,
    force: bool,
    on_file: &dyn Fn(u64, u64),
) -> Result<DefragStats> {
    let start = std::time::Instant::now();

    // ── 1. Open source, read metadata ──
    let reader = ContainerReader::open(sbox_path, kek)?;
    let entries = reader.list_files()?;
    let frag_before = reader.bitmap().fragmentation_ratio();
    let allocated_before = reader.bitmap().allocated_pages();

    if entries.is_empty() {
        return Ok(DefragStats {
            files_moved: 0,
            pages_before: 0,
            pages_after: 0,
            space_reclaimed_pages: 0,
            fragmentation_before: 0.0,
            fragmentation_after: 0.0,
            duration_secs: start.elapsed().as_secs_f64(),
        });
    }

    if !force && frag_before < 0.15 {
        return Ok(DefragStats {
            files_moved: 0,
            pages_before: allocated_before,
            pages_after: allocated_before,
            space_reclaimed_pages: 0,
            fragmentation_before: frag_before,
            fragmentation_after: frag_before,
            duration_secs: start.elapsed().as_secs_f64(),
        });
    }

    // ── 2. Read all files from source, verify CRC32 ──
    let mut files: Vec<(FileEntry, Vec<u8>)> = Vec::with_capacity(entries.len());

    for entry in &entries {
        if entry.is_directory {
            continue;
        }
        let data = reader.read_file(entry)?;
        let computed_crc = crc32(&data);
        if computed_crc != entry.crc32 {
            return Err(SesameError::Corruption(format!(
                "CRC32 mismatch reading '{}': stored={:08x}, computed={:08x}",
                entry.path, entry.crc32, computed_crc
            )));
        }
        files.push((entry.clone(), data));
    }

    // ── 3. Create destination ──
    let dest_path = sbox_path.with_extension("sesame.DEFRAG");

    // Clean up any orphaned defrag from a previous crash
    let _ = fs::remove_file(&dest_path);

    let header = reader.header();
    let salt =
        Salt::from_slice(&header.salt).map_err(|e| SesameError::Crypto(format!("salt: {e}")))?;

    let total_plaintext: u64 = files.iter().map(|(_, d)| d.len() as u64).sum();
    let total_pages = FileEntry::pages_for_size(total_plaintext);
    let data_size = (total_pages as usize) * crate::container::format::PAGE_PLAINTEXT;

    let mut writer = ContainerWriter::create(&dest_path, kek, salt, data_size)?;

    // ── 4. Write all files to dest ──
    // Sort by path for deterministic output
    files.sort_by(|a, b| a.0.path.cmp(&b.0.path));

    let total = files.len() as u64;
    for (index, (entry, data)) in files.iter().enumerate() {
        writer.add_file_data(
            data,
            &entry.path,
            entry.original_path.as_deref().map(Path::new),
            entry.has_symlink,
        )?;
        on_file(index as u64 + 1, total);
    }

    for entry in entries.iter().filter(|entry| entry.is_directory) {
        writer.add_directory_entry(entry.clone())?;
    }

    // ── 5. Close & re-verify ──
    drop(writer); // flush + close

    let verifier = ContainerReader::open(&dest_path, kek)?;
    let dest_entries = verifier.list_files()?;

    if dest_entries.len() != entries.len() {
        let _ = fs::remove_file(&dest_path);
        return Err(SesameError::Corruption(format!(
            "defrag: entry count mismatch after write: expected {}, got {}",
            entries.len(),
            dest_entries.len()
        )));
    }

    for src_entry in &entries {
        if src_entry.is_directory {
            let dest_entry = dest_entries
                .iter()
                .find(|entry| entry.path == src_entry.path)
                .ok_or_else(|| {
                    let _ = fs::remove_file(&dest_path);
                    SesameError::Corruption(format!(
                        "defrag: directory '{}' missing from dest after write",
                        src_entry.path
                    ))
                })?;
            if !dest_entry.is_directory {
                let _ = fs::remove_file(&dest_path);
                return Err(SesameError::Corruption(format!(
                    "defrag: path '{}' is not a directory in dest",
                    src_entry.path
                )));
            }
            continue;
        }
        let dest_entry = dest_entries
            .iter()
            .find(|entry| entry.path == src_entry.path)
            .ok_or_else(|| {
                let _ = fs::remove_file(&dest_path);
                SesameError::Corruption(format!(
                    "defrag: file '{}' missing from dest after write",
                    src_entry.path
                ))
            })?;

        // Verify CRC32 on disk
        let dest_data = verifier.read_file(dest_entry)?;
        let dest_crc = crc32(&dest_data);

        if dest_crc != src_entry.crc32 {
            let _ = fs::remove_file(&dest_path);
            return Err(SesameError::Corruption(format!(
                "defrag: CRC32 mismatch for '{}' after write: expected {:08x}, got {:08x}",
                src_entry.path, src_entry.crc32, dest_crc
            )));
        }
    }

    let frag_after = verifier.bitmap().fragmentation_ratio();
    let allocated_after = verifier.bitmap().allocated_pages();
    drop(verifier);
    // Release the source lock before the rename. A handle kept open across
    // the swap would stay attached to the old file after it is replaced.
    drop(reader);

    // ── 6. Atomic swap ──
    // fsync dest directory before rename
    let dest_file = fs::OpenOptions::new().write(true).open(&dest_path)?;
    dest_file.sync_all()?;
    drop(dest_file);

    #[cfg(windows)]
    {
        let backup_path = sbox_path.with_extension("sesame.DEFRAG-BACKUP");
        let _ = fs::remove_file(&backup_path);
        fs::rename(sbox_path, &backup_path)?;
        if let Err(error) = fs::rename(&dest_path, sbox_path) {
            let _ = fs::rename(&backup_path, sbox_path);
            return Err(error.into());
        }
        fs::remove_file(backup_path)?;
    }
    #[cfg(not(windows))]
    fs::rename(&dest_path, sbox_path)?;

    let space_reclaimed = allocated_before.saturating_sub(allocated_after);

    Ok(DefragStats {
        files_moved: files.len(),
        pages_before: allocated_before,
        pages_after: allocated_after,
        space_reclaimed_pages: space_reclaimed,
        fragmentation_before: frag_before,
        fragmentation_after: frag_after,
        duration_secs: start.elapsed().as_secs_f64(),
    })
}

/// Clean up orphaned .sesame.DEFRAG files.
pub fn cleanup_orphaned_defrag(sbox_path: &Path) {
    let defrag_path = sbox_path.with_extension("sesame.DEFRAG");
    if defrag_path.exists() {
        let _ = fs::remove_file(&defrag_path);
    }
}
