//! Read-only container access.

use crate::container::format::{
    BLOCK_SIZE, ContainerHeader, HEADER_SIZE, PAGE_PLAINTEXT, PAGES_PER_BLOCK,
};
use crate::container::lock::VaultLock;
use crate::container::metadata::{AllocationBitmap, FileEntry, decode_bitmap, decode_file_table};
use crate::container::storage::{block_offset, read_full_block, read_page};
use crate::crypto::Key;
use crate::error::{Result, SesameError};
use crate::keymgmt::KeyEncryptingKey;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub struct ContainerReader {
    path: PathBuf,
    header: ContainerHeader,
    master_key: Key,
    bitmap: AllocationBitmap,
    total_data_pages: u64,
    _lock: VaultLock,
}

impl ContainerReader {
    pub fn open(path: &Path, kek: &KeyEncryptingKey) -> Result<Self> {
        let lock = VaultLock::try_acquire(path)?;
        let mut file = File::open(path)?;
        let file_size = file.metadata()?.len() as usize;
        if file_size < HEADER_SIZE * 3 + 3 * BLOCK_SIZE {
            return Err(SesameError::Corruption(
                "file too small for V3 container".into(),
            ));
        }
        let header = Self::read_any_header(&mut file, file_size)?;
        if kek.config() != header.kdf_config {
            return Err(SesameError::InvalidInput(
                "credential KDF configuration does not match the container header".into(),
            ));
        }
        let master_key = kek
            .unwrap_master_key(&header.wrapped_master_key, &header.nonce)
            .map_err(|_| SesameError::WrongPassword)?;
        let blocks = (file_size - HEADER_SIZE) / BLOCK_SIZE;
        let total_data_pages = blocks.saturating_sub(2) as u64 * PAGES_PER_BLOCK as u64;
        let bitmap = decode_bitmap(&read_full_block(&mut file, block_offset(1), &master_key)?)?;
        if bitmap.total_pages != total_data_pages {
            return Err(SesameError::Corruption(format!(
                "allocation bitmap declares {} pages, container provides {total_data_pages}",
                bitmap.total_pages
            )));
        }
        Ok(Self {
            path: path.to_path_buf(),
            header,
            master_key,
            bitmap,
            total_data_pages,
            _lock: lock,
        })
    }

    pub fn read_header(path: &Path) -> Result<ContainerHeader> {
        let mut file = File::open(path)?;
        let size = file.metadata()?.len() as usize;
        Self::read_any_header(&mut file, size)
    }

    fn read_any_header(file: &mut File, file_size: usize) -> Result<ContainerHeader> {
        let mut last_error = None;
        for offset in [
            0,
            HEADER_SIZE as u64,
            file_size.saturating_sub(HEADER_SIZE) as u64,
        ] {
            let mut bytes = [0; HEADER_SIZE];
            if file.seek(SeekFrom::Start(offset)).is_ok() && file.read_exact(&mut bytes).is_ok() {
                match ContainerHeader::from_bytes(&bytes) {
                    Ok(header) => return Ok(header),
                    Err(error @ SesameError::UnsupportedVersion { .. }) => return Err(error),
                    Err(error) => last_error = Some(error),
                }
            }
        }
        Err(last_error.unwrap_or_else(|| {
            SesameError::Corruption("all three header copies are corrupt or unreadable".into())
        }))
    }

    pub fn header(&self) -> &ContainerHeader {
        &self.header
    }

    pub fn bitmap(&self) -> &AllocationBitmap {
        &self.bitmap
    }

    pub fn total_data_pages(&self) -> u64 {
        self.total_data_pages
    }

    pub(crate) fn master_key(&self) -> &Key {
        &self.master_key
    }

    pub fn list_files(&self) -> Result<Vec<FileEntry>> {
        let mut file = File::open(&self.path)?;
        decode_file_table(&read_full_block(
            &mut file,
            block_offset(0),
            &self.master_key,
        )?)
    }

    pub fn read_page(&self, page_idx: u64) -> Result<Vec<u8>> {
        read_page(&mut File::open(&self.path)?, &self.master_key, page_idx)
    }

    pub fn read_file(&self, entry: &FileEntry) -> Result<Vec<u8>> {
        let mut data = Vec::with_capacity(entry.size as usize);
        for index in 0..entry.page_count {
            let page = self.read_page(entry.start_page + index)?;
            let len = if index + 1 == entry.page_count {
                (entry.size as usize - data.len()).min(page.len())
            } else {
                PAGE_PLAINTEXT
            };
            data.extend_from_slice(&page[..len]);
        }
        Ok(data)
    }

    pub fn verify(&self) -> Result<()> {
        let mut file = File::open(&self.path)?;
        let mut damaged = Vec::new();
        for (block, label) in [(0, "File Table"), (1, "Bitmap")] {
            if read_full_block(&mut file, block_offset(block), &self.master_key).is_err() {
                damaged.push(format!("Block {block} ({label})"));
            }
        }
        for page in 0..self.total_data_pages {
            if self.bitmap.is_allocated(page)
                && read_page(&mut file, &self.master_key, page).is_err()
            {
                damaged.push(format!("Page {page}"));
            }
        }
        if damaged.is_empty() {
            Ok(())
        } else {
            Err(SesameError::Corruption(format!(
                "{} item(s) damaged: {damaged:?}",
                damaged.len()
            )))
        }
    }
}
