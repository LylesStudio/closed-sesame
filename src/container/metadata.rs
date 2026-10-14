//! In-container metadata types and their V3 wire encoding.

use crate::container::format::{BLOCK_SIZE, NONCE_SIZE, PAGE_PLAINTEXT, TAG_SIZE};
use crate::error::{Result, SesameError};

const METADATA_MAGIC: &[u8; 4] = b"CSM3";
const METADATA_ENCODING_VERSION: u8 = 1;
const METADATA_HEADER_SIZE: usize = 12;
const FILE_TABLE_KIND: u8 = 1;
const BITMAP_KIND: u8 = 2;
const MAX_METADATA_SIZE: usize = BLOCK_SIZE - NONCE_SIZE - TAG_SIZE;

fn encode_metadata<T: serde::Serialize>(value: &T, kind: u8) -> Result<Vec<u8>> {
    let payload = postcard::to_allocvec(value)
        .map_err(|error| SesameError::Serialization(error.to_string()))?;
    let encoded_len = METADATA_HEADER_SIZE
        .checked_add(payload.len())
        .ok_or_else(|| SesameError::Serialization("metadata length overflow".into()))?;
    if encoded_len > MAX_METADATA_SIZE {
        return Err(SesameError::Serialization(format!(
            "metadata requires {encoded_len} bytes, block capacity is {MAX_METADATA_SIZE} bytes"
        )));
    }
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| SesameError::Serialization("metadata payload exceeds u32 length".into()))?;
    let mut encoded = Vec::with_capacity(encoded_len);
    encoded.extend_from_slice(METADATA_MAGIC);
    encoded.push(kind);
    encoded.push(METADATA_ENCODING_VERSION);
    encoded.extend_from_slice(&[0; 2]);
    encoded.extend_from_slice(&payload_len.to_be_bytes());
    encoded.extend_from_slice(&payload);
    Ok(encoded)
}

fn decode_metadata<T: serde::de::DeserializeOwned>(bytes: &[u8], expected_kind: u8) -> Result<T> {
    if bytes.len() < METADATA_HEADER_SIZE {
        return Err(SesameError::Corruption(
            "metadata block is shorter than its encoding header".into(),
        ));
    }
    if &bytes[..4] != METADATA_MAGIC {
        return Err(SesameError::Corruption(
            "metadata encoding magic mismatch".into(),
        ));
    }
    if bytes[4] != expected_kind {
        return Err(SesameError::Corruption(format!(
            "metadata kind {} does not match expected kind {expected_kind}",
            bytes[4]
        )));
    }
    if bytes[5] != METADATA_ENCODING_VERSION {
        return Err(SesameError::Corruption(format!(
            "unsupported metadata encoding version {}",
            bytes[5]
        )));
    }
    if bytes[6..8] != [0; 2] {
        return Err(SesameError::Corruption(
            "metadata encoding reserved bytes are non-zero".into(),
        ));
    }
    let payload_len_bytes: [u8; 4] = bytes[8..12]
        .try_into()
        .map_err(|_| SesameError::Corruption("invalid metadata length field".into()))?;
    let payload_len = u32::from_be_bytes(payload_len_bytes) as usize;
    let payload_end = METADATA_HEADER_SIZE
        .checked_add(payload_len)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| SesameError::Corruption("metadata payload length exceeds block".into()))?;
    if bytes[payload_end..].iter().any(|byte| *byte != 0) {
        return Err(SesameError::Corruption(
            "metadata block has non-zero bytes after its declared payload".into(),
        ));
    }
    postcard::from_bytes(&bytes[METADATA_HEADER_SIZE..payload_end])
        .map_err(|error| SesameError::Corruption(format!("invalid metadata payload: {error}")))
}

pub fn encode_file_table(entries: &[FileEntry]) -> Result<Vec<u8>> {
    encode_metadata(&entries, FILE_TABLE_KIND)
}

pub fn decode_file_table(bytes: &[u8]) -> Result<Vec<FileEntry>> {
    decode_metadata(bytes, FILE_TABLE_KIND)
}

pub fn encode_bitmap(bitmap: &AllocationBitmap) -> Result<Vec<u8>> {
    encode_metadata(bitmap, BITMAP_KIND)
}

pub fn decode_bitmap(bytes: &[u8]) -> Result<AllocationBitmap> {
    let bitmap: AllocationBitmap = decode_metadata(bytes, BITMAP_KIND)?;
    let expected_len = usize::try_from(bitmap.total_pages.div_ceil(8))
        .map_err(|_| SesameError::Corruption("allocation bitmap is too large".into()))?;
    if bitmap.bits.len() != expected_len {
        return Err(SesameError::Corruption(format!(
            "allocation bitmap has {} bytes, expected {expected_len}",
            bitmap.bits.len()
        )));
    }
    let used_bits = bitmap.total_pages % 8;
    if used_bits != 0
        && bitmap
            .bits
            .last()
            .is_some_and(|byte| byte & !((1u8 << used_bits) - 1) != 0)
    {
        return Err(SesameError::Corruption(
            "allocation bitmap has set bits beyond its declared page count".into(),
        ));
    }
    Ok(bitmap)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContainerFlags(u16);

impl ContainerFlags {
    pub const NONE: Self = Self(0);
    pub const PASSWORD_PROTECTED: u16 = 1 << 0;
    pub const OBFUSCATED_SYMLINKS: u16 = 1 << 1;

    pub fn is_password_protected(&self) -> bool {
        self.0 & Self::PASSWORD_PROTECTED != 0
    }

    pub fn set_password_protected(&mut self, value: bool) {
        if value {
            self.0 |= Self::PASSWORD_PROTECTED;
        } else {
            self.0 &= !Self::PASSWORD_PROTECTED;
        }
    }

    pub fn to_u16(self) -> u16 {
        self.0
    }

    pub fn from_u16(value: u16) -> Self {
        Self(value)
    }
}

impl Default for ContainerFlags {
    fn default() -> Self {
        Self::NONE
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AllocationBitmap {
    pub bits: Vec<u8>,
    pub total_pages: u64,
}

impl AllocationBitmap {
    pub fn new(total_pages: u64) -> Self {
        Self {
            bits: vec![0; total_pages.div_ceil(8) as usize],
            total_pages,
        }
    }

    pub fn is_allocated(&self, page_idx: u64) -> bool {
        page_idx < self.total_pages
            && self.bits[page_idx as usize >> 3] & (1 << (page_idx & 7)) != 0
    }

    pub fn allocate(&mut self, page_idx: u64) {
        if page_idx < self.total_pages {
            self.bits[page_idx as usize >> 3] |= 1 << (page_idx & 7);
        }
    }

    pub fn free(&mut self, page_idx: u64) {
        if page_idx < self.total_pages {
            self.bits[page_idx as usize >> 3] &= !(1 << (page_idx & 7));
        }
    }

    pub fn find_contiguous(&self, count: u64) -> Option<u64> {
        if count == 0 || count > self.total_pages {
            return None;
        }
        let (mut start, mut len) = (None, 0);
        for page in 0..self.total_pages {
            if self.is_allocated(page) {
                start = None;
                len = 0;
            } else {
                start.get_or_insert(page);
                len += 1;
                if len == count {
                    return start;
                }
            }
        }
        None
    }

    pub fn free_pages(&self) -> u64 {
        self.bits
            .iter()
            .map(|byte| 8 - byte.count_ones() as u64)
            .sum::<u64>()
            .min(self.total_pages)
    }

    pub fn allocated_pages(&self) -> u64 {
        self.total_pages.saturating_sub(self.free_pages())
    }

    pub fn fragmentation_ratio(&self) -> f64 {
        let free = self.free_pages();
        if free == 0 {
            return 0.0;
        }
        let (mut largest, mut current) = (0, 0);
        for page in 0..self.total_pages {
            if self.is_allocated(page) {
                current = 0;
            } else {
                current += 1;
                largest = largest.max(current);
            }
        }
        1.0 - largest as f64 / free as f64
    }

    pub fn byte_size(&self) -> usize {
        self.bits.len()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub original_path: Option<String>,
    pub size: u64,
    pub start_page: u64,
    pub page_count: u64,
    pub crc32: u32,
    pub created_at: u64,
    pub modified_at: u64,
    pub has_symlink: bool,
    pub is_directory: bool,
}

impl FileEntry {
    pub fn pages_for_size(size: u64) -> u64 {
        if size == 0 {
            1
        } else {
            size.div_ceil(PAGE_PLAINTEXT as u64)
        }
    }
}
