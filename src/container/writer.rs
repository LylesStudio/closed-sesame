//! Mutable container access.

use crate::container::checksum::crc32;
use crate::container::format::{
    BLOCK_SIZE, ContainerHeader, HEADER_SIZE, PAGE_PLAINTEXT, PAGE_SIZE, PAGES_PER_BLOCK,
    WRAPPED_KEY_SIZE,
};
use crate::container::lock::VaultLock;
use crate::container::metadata::{
    AllocationBitmap, ContainerFlags, FileEntry, encode_bitmap, encode_file_table,
};
use crate::container::path::{ancestors, normalize};
use crate::container::reader::ContainerReader;
use crate::container::storage::{block_offset, read_page, write_full_block, write_page};
use crate::crypto::{self, Key, Salt, TAG_SIZE};
use crate::error::{Result, SesameError};
use crate::i18n::keys;
use crate::keymgmt::{KeyEncryptingKey, KeyMode};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub struct ContainerWriter {
    file: File,
    master_key: Key,
    bitmap: AllocationBitmap,
    entries: Vec<FileEntry>,
    _lock: VaultLock,
}

impl ContainerWriter {
    pub fn create(
        path: &Path,
        kek: &KeyEncryptingKey,
        salt: Salt,
        data_size: usize,
    ) -> Result<Self> {
        if path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("{} already exists", path.display()),
            )
            .into());
        }
        let master_key = crypto::random_key();
        let (wrapped, nonce) = kek.wrap_master_key(&master_key)?;
        let wrapped: [u8; WRAPPED_KEY_SIZE] = wrapped
            .try_into()
            .map_err(|_| SesameError::Crypto("wrapped master key has unexpected length".into()))?;
        let mut flags = ContainerFlags::NONE;
        flags.set_password_protected(kek.mode() == KeyMode::PasswordProtected);
        let header = ContainerHeader::new(
            flags,
            *salt.as_bytes(),
            nonce,
            [0; TAG_SIZE],
            wrapped,
            kek.config(),
        )?;
        let header_bytes = header.to_bytes()?;

        let requested_pages = (data_size as u64).div_ceil(PAGE_PLAINTEXT as u64).max(1);
        let data_blocks = requested_pages.div_ceil(PAGES_PER_BLOCK as u64);
        let bitmap = AllocationBitmap::new(data_blocks * PAGES_PER_BLOCK as u64);
        let file_size = HEADER_SIZE as u64 * 3 + (2 + data_blocks) * BLOCK_SIZE as u64;
        let mut file = File::create_new(path)?;
        let lock = match VaultLock::try_acquire(path) {
            Ok(lock) => lock,
            Err(error) => {
                drop(file);
                let _ = std::fs::remove_file(path);
                return Err(error);
            }
        };
        file.write_all(&header_bytes)?;
        file.write_all(&header_bytes)?;
        write_full_block(
            &mut file,
            block_offset(0),
            &master_key,
            &encode_file_table(&[])?,
        )?;
        write_full_block(
            &mut file,
            block_offset(1),
            &master_key,
            &encode_bitmap(&bitmap)?,
        )?;
        let zero_page = vec![0; PAGE_SIZE];
        for block in 0..data_blocks {
            file.seek(SeekFrom::Start(block_offset(2 + block)))?;
            for _ in 0..PAGES_PER_BLOCK {
                file.write_all(&zero_page)?;
            }
        }
        file.seek(SeekFrom::Start(file_size - HEADER_SIZE as u64))?;
        file.write_all(&header_bytes)?;
        file.sync_all()?;
        Ok(Self {
            file,
            master_key,
            bitmap,
            entries: Vec::new(),
            _lock: lock,
        })
    }

    pub fn open(path: &Path, kek: &KeyEncryptingKey) -> Result<Self> {
        let lock = VaultLock::try_acquire(path)?;
        let reader = ContainerReader::open(path, kek)?;
        let master_key = Key::from_slice(reader.master_key().as_bytes())?;
        let bitmap = reader.bitmap().clone();
        let entries = reader.list_files()?;
        validate_entry_tree(&entries)?;
        Ok(Self {
            file: OpenOptions::new().read(true).write(true).open(path)?,
            master_key,
            bitmap,
            entries,
            _lock: lock,
        })
    }

    pub fn bitmap(&self) -> &AllocationBitmap {
        &self.bitmap
    }

    pub fn entries(&self) -> &[FileEntry] {
        &self.entries
    }

    pub fn find_entry(&self, target_path: &str) -> Option<&FileEntry> {
        self.find_index(target_path)
            .map(|index| &self.entries[index])
    }

    fn allocate_pages(&mut self, count: u64) -> Result<u64> {
        let start = self
            .bitmap
            .find_contiguous(count)
            .ok_or(SesameError::DiskFull {
                needed: count * PAGE_SIZE as u64,
                available: self.bitmap.free_pages() * PAGE_SIZE as u64,
            })?;
        for page in start..start + count {
            self.bitmap.allocate(page);
        }
        Ok(start)
    }

    fn persist_metadata(&mut self) -> Result<()> {
        self.persist_bitmap()?;
        self.persist_table()?;
        Ok(())
    }

    /// Durable write of the allocation bitmap only.
    fn persist_bitmap(&mut self) -> Result<()> {
        write_full_block(
            &mut self.file,
            block_offset(1),
            &self.master_key,
            &encode_bitmap(&self.bitmap)?,
        )?;
        self.file.sync_data()?;
        Ok(())
    }

    /// Durable write of the file table only.
    fn persist_table(&mut self) -> Result<()> {
        write_full_block(
            &mut self.file,
            block_offset(0),
            &self.master_key,
            &encode_file_table(&self.entries)?,
        )?;
        self.file.sync_data()?;
        Ok(())
    }

    fn now_secs() -> Result<u64> {
        Ok(std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| SesameError::SystemClock(error.to_string()))?
            .as_secs())
    }

    fn find_index(&self, target_path: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.path == target_path)
    }

    pub fn mkdir(&mut self, raw_path: &str) -> Result<FileEntry> {
        let path = normalize(raw_path)?;
        self.ensure_parent_directories(&path)?;
        if let Some(index) = self.find_index(&path) {
            return if self.entries[index].is_directory {
                Ok(self.entries[index].clone())
            } else {
                Err(SesameError::InvalidInput(format!(
                    "cannot create directory at existing file path: {path}"
                )))
            };
        }
        let entry = Self::directory_entry(&path, None, Self::now_secs()?);
        self.entries.push(entry.clone());
        self.persist_metadata()?;
        Ok(entry)
    }

    pub fn mark_directory_symlink(
        &mut self,
        raw_path: &str,
        original_path: &Path,
    ) -> Result<FileEntry> {
        let path = normalize(raw_path)?;
        self.ensure_parent_directories(&path)?;
        if let Some(index) = self.find_index(&path) {
            if !self.entries[index].is_directory {
                return Err(SesameError::InvalidInput(format!(
                    "cannot mark symlink on file path: {path}"
                )));
            }
            self.entries[index].original_path = Some(original_path.to_string_lossy().into_owned());
            self.entries[index].has_symlink = true;
            let entry = self.entries[index].clone();
            self.persist_metadata()?;
            Ok(entry)
        } else {
            let mut entry = Self::directory_entry(&path, Some(original_path), Self::now_secs()?);
            entry.has_symlink = true;
            self.entries.push(entry.clone());
            self.persist_metadata()?;
            Ok(entry)
        }
    }

    pub fn add_directory_entry(&mut self, entry: FileEntry) -> Result<()> {
        let path = normalize(&entry.path)?;
        if !entry.is_directory {
            return Err(SesameError::InvalidInput(
                "add_directory_entry requires is_directory=true".into(),
            ));
        }
        if entry.page_count != 0 || entry.size != 0 {
            return Err(SesameError::InvalidInput(
                "directory entries must not allocate data pages".into(),
            ));
        }
        self.ensure_parent_directories(&path)?;
        if let Some(index) = self.find_index(&path) {
            if self.entries[index].is_directory {
                self.entries[index] = FileEntry { path, ..entry };
            } else {
                return Err(SesameError::InvalidInput(format!(
                    "directory path conflicts with file: {}",
                    entry.path
                )));
            }
        } else {
            self.entries.push(FileEntry { path, ..entry });
        }
        self.persist_metadata()?;
        Ok(())
    }

    fn directory_entry(path: &str, original_path: Option<&Path>, now: u64) -> FileEntry {
        FileEntry {
            path: path.to_owned(),
            original_path: original_path.map(|value| value.to_string_lossy().into_owned()),
            size: 0,
            start_page: 0,
            page_count: 0,
            crc32: 0,
            created_at: now,
            modified_at: now,
            has_symlink: false,
            is_directory: true,
        }
    }

    fn ensure_parent_directories(&mut self, path: &str) -> Result<()> {
        for ancestor in ancestors(path) {
            if let Some(index) = self.find_index(&ancestor) {
                if !self.entries[index].is_directory {
                    return Err(SesameError::InvalidInput(format!(
                        "parent path is not a directory: {ancestor}"
                    )));
                }
            } else {
                let now = Self::now_secs()?;
                self.entries
                    .push(Self::directory_entry(&ancestor, None, now));
            }
        }
        Ok(())
    }

    pub fn add_file(&mut self, source_path: &Path, target_name: &str) -> Result<FileEntry> {
        self.add_file_with_symlink(source_path, target_name, false)
    }

    pub fn add_file_with_symlink(
        &mut self,
        source_path: &Path,
        target_name: &str,
        has_symlink: bool,
    ) -> Result<FileEntry> {
        self.add_file_data(
            &std::fs::read(source_path)?,
            target_name,
            Some(source_path),
            has_symlink,
        )
    }

    pub fn add_file_data(
        &mut self,
        data: &[u8],
        target_name: &str,
        original_path: Option<&Path>,
        has_symlink: bool,
    ) -> Result<FileEntry> {
        let path = normalize(target_name)?;
        self.ensure_parent_directories(&path)?;
        let replaced = if let Some(index) = self.find_index(&path) {
            if self.entries[index].is_directory {
                return Err(SesameError::InvalidInput(format!(
                    "cannot add file over directory: {path}"
                )));
            }
            Some((
                self.entries[index].start_page,
                self.entries[index].page_count,
            ))
        } else {
            None
        };

        let page_count = FileEntry::pages_for_size(data.len() as u64);
        let start_page = self.write_new_pages(data, page_count)?;
        let now = Self::now_secs()?;
        let entry = FileEntry {
            path,
            original_path: original_path.map(|value| value.to_string_lossy().into_owned()),
            size: data.len() as u64,
            start_page,
            page_count,
            crc32: crc32(data),
            created_at: now,
            modified_at: now,
            has_symlink,
            is_directory: false,
        };
        if let Some(index) = self.find_index(&entry.path) {
            self.entries[index] = entry.clone();
        } else {
            self.entries.push(entry.clone());
        }
        self.persist_metadata()?;
        self.release_replaced_pages(replaced)?;
        Ok(entry)
    }

    /// Replace an existing file's contents while preserving its identity
    /// metadata (`created_at`, `original_path`, `has_symlink`).
    ///
    /// Uses the same two-phase commit as [`add_file_data`]: new pages are
    /// written and committed before the superseded pages are released, so a
    /// crash can only leak pages, never corrupt the previous version.
    pub fn replace_file_data(&mut self, target_name: &str, data: &[u8]) -> Result<FileEntry> {
        let path = normalize(target_name)?;
        let Some(index) = self.find_index(&path) else {
            return Err(SesameError::InvalidInput(format!("path not found: {path}")));
        };
        if self.entries[index].is_directory {
            return Err(SesameError::InvalidInput(format!(
                "cannot replace directory with file data: {path}"
            )));
        }
        let old = self.entries[index].clone();
        let page_count = FileEntry::pages_for_size(data.len() as u64);
        let start_page = self.write_new_pages(data, page_count)?;
        let entry = FileEntry {
            size: data.len() as u64,
            start_page,
            page_count,
            crc32: crc32(data),
            modified_at: Self::now_secs()?,
            ..old.clone()
        };
        self.entries[index] = entry.clone();
        self.persist_metadata()?;
        self.release_replaced_pages(Some((old.start_page, old.page_count)))?;
        Ok(entry)
    }

    /// Allocate fresh pages and write `data` into them. On failure the
    /// allocation is rolled back so the writer stays consistent.
    fn write_new_pages(&mut self, data: &[u8], page_count: u64) -> Result<u64> {
        let start_page = self.allocate_pages(page_count)?;
        let write_result = (|| -> Result<()> {
            for index in 0..page_count {
                let start = index as usize * PAGE_PLAINTEXT;
                let end = (start + PAGE_PLAINTEXT).min(data.len());
                write_page(
                    &mut self.file,
                    &self.master_key,
                    start_page + index,
                    if start < data.len() {
                        &data[start..end]
                    } else {
                        &[]
                    },
                )?;
            }
            self.file.sync_data()?;
            Ok(())
        })();
        if let Err(error) = write_result {
            for page in start_page..start_page + page_count {
                self.bitmap.free(page);
            }
            return Err(error);
        }
        Ok(start_page)
    }

    /// Free superseded pages after the replacement entry is durable.
    fn release_replaced_pages(&mut self, replaced: Option<(u64, u64)>) -> Result<()> {
        let Some((start_page, page_count)) = replaced else {
            return Ok(());
        };
        for page in start_page..start_page + page_count {
            self.bitmap.free(page);
        }
        self.persist_bitmap()
    }

    pub fn remove_file(&mut self, target_name: &str) -> Result<FileEntry> {
        let removed = self.remove_path(target_name)?;
        removed
            .into_iter()
            .find(|entry| entry.path == normalize(target_name).unwrap_or_default())
            .ok_or_else(|| {
                SesameError::Corruption(format!("path '{target_name}' not found after removal"))
            })
    }

    pub fn remove_path(&mut self, raw_path: &str) -> Result<Vec<FileEntry>> {
        let path = normalize(raw_path)?;
        let Some(index) = self.find_index(&path) else {
            return Err(SesameError::InvalidInput(format!("path not found: {path}")));
        };
        let is_directory = self.entries[index].is_directory;
        let mut removed_ranges = Vec::new();
        let mut removed = Vec::new();
        if is_directory {
            let prefix = format!("{path}/");
            let mut indices: Vec<usize> = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.path == path || entry.path.starts_with(&prefix))
                .map(|(index, _)| index)
                .collect();
            indices.sort_unstable_by(|left, right| right.cmp(left));
            for index in indices {
                let entry = self.entries.remove(index);
                if !entry.is_directory {
                    removed_ranges.push((entry.start_page, entry.page_count));
                }
                removed.push(entry);
            }
        } else {
            let entry = self.entries.remove(index);
            if !entry.is_directory {
                removed_ranges.push((entry.start_page, entry.page_count));
            }
            removed.push(entry);
        }
        // Commit the table without the removed entries first; only then mark
        // their pages free. A crash between the two steps leaks pages instead
        // of leaving the table pointing at pages the bitmap considers free.
        self.persist_table()?;
        for (start_page, page_count) in removed_ranges {
            for page in start_page..start_page + page_count {
                self.bitmap.free(page);
            }
        }
        self.persist_bitmap()?;
        Ok(removed)
    }

    pub fn rename_path(&mut self, raw_old: &str, raw_new: &str) -> Result<()> {
        let old_path = normalize(raw_old)?;
        let new_path = normalize(raw_new)?;
        if old_path == new_path {
            return Ok(());
        }
        if new_path.starts_with(&format!("{old_path}/")) {
            return Err(SesameError::InvalidInput(keys::COPY_INTO_SELF.into()));
        }
        if self.find_index(&old_path).is_none() {
            return Err(SesameError::InvalidInput(format!(
                "path not found: {old_path}"
            )));
        }
        if self.find_index(&new_path).is_some() {
            return Err(SesameError::InvalidInput(format!(
                "destination already exists: {new_path}"
            )));
        }
        self.ensure_parent_directories(&new_path)?;

        let old_prefix = format!("{old_path}/");
        for entry in &mut self.entries {
            if entry.path == old_path {
                entry.path = new_path.clone();
                entry.modified_at = Self::now_secs()?;
            } else if entry.path.starts_with(&old_prefix) {
                entry.path = format!("{new_path}{}", &entry.path[old_path.len()..]);
                entry.modified_at = Self::now_secs()?;
            }
        }

        self.persist_metadata()?;
        Ok(())
    }

    /// Wrap the existing master key with `kek` (derived from `salt`) and
    /// rewrite all three header copies. File pages stay untouched.
    pub fn rekey(&mut self, kek: &KeyEncryptingKey, salt: &Salt) -> Result<()> {
        let (wrapped, nonce) = kek.wrap_master_key(&self.master_key)?;
        let wrapped: [u8; WRAPPED_KEY_SIZE] = wrapped
            .try_into()
            .map_err(|_| SesameError::Crypto("wrapped master key has unexpected length".into()))?;
        let old = self.read_header()?;
        let mut flags = old.flags;
        flags.set_password_protected(kek.mode() == KeyMode::PasswordProtected);
        let mut header = ContainerHeader::new(
            flags,
            *salt.as_bytes(),
            nonce,
            [0; TAG_SIZE],
            wrapped,
            kek.config(),
        )?;
        header.container_id = old.container_id;
        self.write_header_copies(&header.to_bytes()?)
    }

    /// Extend the data area by at least `additional_bytes` of plaintext.
    /// Growth is rounded up to whole 4 MiB data blocks. Returns the new
    /// plaintext capacity.
    pub fn grow(&mut self, additional_bytes: u64) -> Result<u64> {
        if additional_bytes == 0 {
            return Ok(self.capacity_bytes());
        }
        let add_pages = additional_bytes.div_ceil(PAGE_PLAINTEXT as u64);
        let add_blocks = add_pages.div_ceil(PAGES_PER_BLOCK as u64);
        let new_total = self
            .bitmap
            .total_pages
            .saturating_add(add_blocks * PAGES_PER_BLOCK as u64);
        let mut new_bitmap = self.bitmap.clone();
        new_bitmap.total_pages = new_total;
        new_bitmap.bits.resize(new_total.div_ceil(8) as usize, 0);
        encode_bitmap(&new_bitmap)?;

        let old_len = self.file.metadata()?.len();
        let expected_old = file_len_for_pages(self.bitmap.total_pages)?;
        if old_len != expected_old {
            return Err(SesameError::Corruption(format!(
                "container is {old_len} bytes, expected {expected_old}"
            )));
        }
        let new_len = file_len_for_pages(new_total)?;
        let header = {
            let mut bytes = [0u8; HEADER_SIZE];
            self.file.seek(SeekFrom::Start(0))?;
            self.file.read_exact(&mut bytes)?;
            bytes
        };
        self.file
            .seek(SeekFrom::Start(old_len - HEADER_SIZE as u64))?;
        self.file.write_all(&[0u8; HEADER_SIZE])?;
        self.file.set_len(new_len)?;
        self.file
            .seek(SeekFrom::Start(new_len - HEADER_SIZE as u64))?;
        self.file.write_all(&header)?;
        self.file.sync_data()?;
        self.bitmap = new_bitmap;
        self.persist_bitmap()?;
        Ok(self.capacity_bytes())
    }

    pub fn capacity_bytes(&self) -> u64 {
        self.bitmap.total_pages * PAGE_PLAINTEXT as u64
    }

    /// Duplicate a file or directory tree to `raw_to`. The destination must
    /// not already exist, and a directory cannot be copied into itself.
    pub fn copy_path(&mut self, raw_from: &str, raw_to: &str) -> Result<()> {
        let from = normalize(raw_from)?;
        let to = normalize(raw_to)?;
        if from == to || to.starts_with(&format!("{from}/")) {
            return Err(SesameError::InvalidInput(keys::COPY_INTO_SELF.into()));
        }
        if self.find_index(&to).is_some() {
            return Err(SesameError::InvalidInput(keys::DESTINATION_EXISTS.into()));
        }
        let prefix = format!("{from}/");
        let snapshot: Vec<FileEntry> = self
            .entries
            .iter()
            .filter(|entry| entry.path == from || entry.path.starts_with(&prefix))
            .cloned()
            .collect();
        if snapshot.is_empty() {
            return Err(SesameError::InvalidInput(format!("path not found: {from}")));
        }
        let source_is_dir = snapshot
            .iter()
            .any(|entry| entry.path == from && entry.is_directory);
        let mut files = Vec::new();
        for entry in &snapshot {
            if entry.is_directory {
                continue;
            }
            files.push((entry.path.clone(), self.read_entry_bytes(entry)?));
        }
        if source_is_dir {
            self.mkdir(&to)?;
            for entry in snapshot
                .iter()
                .filter(|entry| entry.is_directory && entry.path != from)
            {
                let dest = format!("{to}{}", &entry.path[from.len()..]);
                self.mkdir(&dest)?;
            }
            for (path, data) in files {
                let dest = format!("{to}{}", &path[from.len()..]);
                self.add_file_data(&data, &dest, None, false)?;
            }
        } else {
            let (_, data) = files
                .into_iter()
                .next()
                .ok_or_else(|| SesameError::InvalidInput(format!("path not found: {from}")))?;
            self.add_file_data(&data, &to, None, false)?;
        }
        Ok(())
    }

    fn read_entry_bytes(&mut self, entry: &FileEntry) -> Result<Vec<u8>> {
        let mut data = Vec::with_capacity(entry.size as usize);
        for index in 0..entry.page_count {
            let page = read_page(&mut self.file, &self.master_key, entry.start_page + index)?;
            let len = if index + 1 == entry.page_count {
                (entry.size as usize)
                    .saturating_sub(data.len())
                    .min(page.len())
            } else {
                PAGE_PLAINTEXT.min(page.len())
            };
            data.extend_from_slice(&page[..len]);
        }
        Ok(data)
    }

    fn read_header(&mut self) -> Result<ContainerHeader> {
        let mut bytes = [0u8; HEADER_SIZE];
        self.file.seek(SeekFrom::Start(0))?;
        self.file.read_exact(&mut bytes)?;
        ContainerHeader::from_bytes(&bytes)
    }

    fn write_header_copies(&mut self, bytes: &[u8; HEADER_SIZE]) -> Result<()> {
        let len = self.file.metadata()?.len();
        for offset in [0u64, HEADER_SIZE as u64, len - HEADER_SIZE as u64] {
            self.file.seek(SeekFrom::Start(offset))?;
            self.file.write_all(bytes)?;
        }
        self.file.sync_data()?;
        Ok(())
    }
}

fn file_len_for_pages(total_pages: u64) -> Result<u64> {
    if !total_pages.is_multiple_of(PAGES_PER_BLOCK as u64) {
        return Err(SesameError::Corruption(format!(
            "data page count {total_pages} is not a multiple of {PAGES_PER_BLOCK}"
        )));
    }
    let data_blocks = total_pages / PAGES_PER_BLOCK as u64;
    Ok(HEADER_SIZE as u64 * 3 + (2 + data_blocks) * BLOCK_SIZE as u64)
}

/// Validate the entry tree in O(n · depth): one hash-join pass instead of
/// the old O(n²) pairwise scan (this runs on every container open, including
/// mount, so quadratic behavior made first mounts painfully slow on large
/// vaults).
pub fn validate_entry_tree(entries: &[FileEntry]) -> Result<()> {
    let mut by_path = std::collections::HashMap::with_capacity(entries.len());
    for entry in entries {
        let path = normalize(&entry.path)?;
        if by_path.insert(path.clone(), entry).is_some() {
            return Err(SesameError::Corruption(format!(
                "duplicate container path: {path}"
            )));
        }
    }

    for (path, entry) in &by_path {
        if entry.is_directory {
            if entry.page_count != 0 || entry.size != 0 {
                return Err(SesameError::Corruption(format!(
                    "directory allocates data pages: {path}"
                )));
            }
        } else if path.contains('/') {
            // Files must have a complete chain of directory parents.
            for ancestor in ancestors(path) {
                match by_path.get(ancestor.as_str()) {
                    Some(parent) if parent.is_directory => {}
                    Some(_) => {
                        return Err(SesameError::Corruption(format!(
                            "parent path is not a directory: {ancestor}"
                        )));
                    }
                    None => {
                        return Err(SesameError::Corruption(format!(
                            "orphan path missing parent directory: {path}"
                        )));
                    }
                }
            }
        }
    }

    // No path (file or directory) may live under a file.
    for path in by_path.keys() {
        for ancestor in ancestors(path) {
            if let Some(parent) = by_path.get(ancestor.as_str())
                && !parent.is_directory
            {
                return Err(SesameError::Corruption(format!(
                    "path nested under file: {ancestor}"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Credential, vault};
    use crate::keymgmt::KeyEncryptingKey;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn open_writer(temp: &TempDir) -> (PathBuf, KeyEncryptingKey, ContainerWriter) {
        let path = temp.path().join("vault.sesame");
        vault::create(&path, 1024 * 1024, Credential::NoPassword).unwrap();
        let credentials =
            crate::app::credentials::prepare_credentials(&path, Credential::NoPassword).unwrap();
        let writer = ContainerWriter::open(&path, &credentials).unwrap();
        (path, credentials, writer)
    }

    #[test]
    fn mark_directory_symlink_updates_existing_dir() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("original");
        std::fs::create_dir_all(&source).unwrap();
        let (_path, _credentials, mut writer) = open_writer(&temp);
        writer.mkdir("docs").unwrap();
        let entry = writer.mark_directory_symlink("docs", &source).unwrap();
        assert!(entry.has_symlink);
        assert_eq!(
            entry.original_path.as_deref(),
            Some(source.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn mark_directory_symlink_creates_missing_dir() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("original");
        std::fs::create_dir_all(&source).unwrap();
        let (_path, _credentials, mut writer) = open_writer(&temp);
        let entry = writer.mark_directory_symlink("newdir", &source).unwrap();
        assert!(entry.is_directory);
        assert!(entry.has_symlink);
    }

    #[test]
    fn rekey_rejects_old_password_and_keeps_bytes() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("vault.sesame");
        vault::create(&path, 1024 * 1024, Credential::Password("old-secret")).unwrap();
        let old =
            crate::app::credentials::prepare_credentials(&path, Credential::Password("old-secret"))
                .unwrap();
        let new = vault::change_password(&path, &old, "new-secret").unwrap();
        drop(old);
        ContainerWriter::open(&path, &new)
            .unwrap()
            .add_file_data(b"hello", "note.txt", None, false)
            .unwrap();
        let stale =
            crate::app::credentials::prepare_credentials(&path, Credential::Password("old-secret"))
                .unwrap();
        assert!(ContainerReader::open(&path, &stale).is_err());
        let reader = ContainerReader::open(&path, &new).unwrap();
        let entry = reader
            .list_files()
            .unwrap()
            .into_iter()
            .find(|entry| entry.path == "note.txt")
            .unwrap();
        assert_eq!(reader.read_file(&entry).unwrap(), b"hello");
    }

    #[test]
    fn rekey_to_no_password_rejects_the_old_password() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("vault.sesame");
        vault::create(&path, 1024 * 1024, Credential::Password("old-secret")).unwrap();
        let old =
            crate::app::credentials::prepare_credentials(&path, Credential::Password("old-secret"))
                .unwrap();
        vault::clear_password(&path, &old).unwrap();
        drop(old);
        assert!(
            crate::app::credentials::prepare_credentials(&path, Credential::Password("old-secret"))
                .is_err()
        );
        let opened =
            crate::app::credentials::prepare_credentials(&path, Credential::NoPassword).unwrap();
        let reader = ContainerReader::open(&path, &opened).unwrap();
        assert!(reader.list_files().unwrap().is_empty());
    }

    #[test]
    fn grow_extends_capacity_without_losing_files() {
        let temp = TempDir::new().unwrap();
        let (path, credentials, mut writer) = open_writer(&temp);
        let before = writer.capacity_bytes();
        writer.add_file_data(b"abc", "a.txt", None, false).unwrap();
        let after = writer.grow(8 * 1024 * 1024).unwrap();
        assert!(after > before);
        drop(writer);
        let reader = ContainerReader::open(&path, &credentials).unwrap();
        assert!(
            reader.total_data_pages() > before / crate::container::format::PAGE_PLAINTEXT as u64
        );
        let entry = reader
            .list_files()
            .unwrap()
            .into_iter()
            .find(|entry| entry.path == "a.txt")
            .unwrap();
        assert_eq!(reader.read_file(&entry).unwrap(), b"abc");
    }

    #[test]
    fn copy_duplicates_file_and_directory() {
        let temp = TempDir::new().unwrap();
        let (_path, _credentials, mut writer) = open_writer(&temp);
        writer
            .add_file_data(b"abc", "docs/a.txt", None, false)
            .unwrap();
        writer.copy_path("docs/a.txt", "docs/b.txt").unwrap();
        writer.copy_path("docs", "backup").unwrap();
        assert_eq!(writer.find_entry("docs/b.txt").unwrap().size, 3);
        assert!(writer.find_entry("backup/a.txt").is_some());
        assert!(writer.find_entry("backup/b.txt").is_some());
        assert!(writer.copy_path("docs", "docs/nested").is_err());
    }
}
