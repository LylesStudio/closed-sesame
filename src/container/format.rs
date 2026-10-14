//! Container header — fixed-size binary format (256 bytes).
//!
//! Header layout (big-endian unless noted):
//! ```text
//! offset  size  field
//! 0       16    magic: b"SESAMEBOX\0\0\0\0\0\0\0"
//! 16      2     version: u16 (currently 3)
//! 18      2     flags: u16
//! 20      32    salt: [u8; 32]
//! 52      12    nonce: [u8; 12]
//! 64      16    tag: [u8; 16] (reserved)
//! 80      48    wrapped_master_key: [u8; 48]
//! 128     16    container_id: UUID bytes
//! 144      2    KDF algorithm
//! 146      2    KDF schema version
//! 148      4    KDF memory_kib
//! 152      4    KDF iterations
//! 156      4    KDF parallelism
//! 160      4    KDF output_len
//! 164     92    reserved (must be zero)
//! total: 256
//! ```
//!
//! V3 data layout (sub-block allocation):
//! ```text
//! Block 0       → File Table (full-block GCM, up to 4MB)
//! Block 1       → Allocation Bitmap (full-block GCM, up to 4MB)
//! Blocks 2..N   → Data pages, each 4KB, per-page GCM:
//!                  [nonce:12][encrypted 4068B + tag:16]
//! ```
//!
//! All fields are fixed-size. No serde needed — manual byte packing.

pub use crate::container::metadata::{AllocationBitmap, ContainerFlags, FileEntry};
pub use crate::crypto::{KdfAlgorithm, KdfConfig, NONCE_SIZE, SALT_SIZE, TAG_SIZE};
use crate::error::{Result, SesameError};
use uuid::Uuid;

/// Magic bytes
pub const MAGIC: &[u8; 16] = b"SESAMEBOX\0\0\0\0\0\0\0";

/// Header size in bytes
pub const HEADER_SIZE: usize = 256;

/// Current format version (V3 persists a self-described KDF configuration).
pub const CURRENT_VERSION: u16 = 3;

/// Block size in bytes (4MB)
pub const BLOCK_SIZE: usize = 4 * 1024 * 1024;

/// Page size in bytes (4KB)
pub const PAGE_SIZE: usize = 4096;

/// Pages per data block
pub const PAGES_PER_BLOCK: usize = BLOCK_SIZE / PAGE_SIZE; // 1024

/// Bitmap bits per block (1 bit = 1 page)
pub const BITMAP_BITS_PER_BLOCK: usize = PAGES_PER_BLOCK;

/// Bitmap bytes per block
pub const BITMAP_BYTES_PER_BLOCK: usize = BITMAP_BITS_PER_BLOCK / 8; // 128

/// Usable plaintext bytes per page (after nonce + GCM tag)
pub const PAGE_PLAINTEXT: usize = PAGE_SIZE - NONCE_SIZE - TAG_SIZE; // 4068

/// Maximum cached blocks
pub const MAX_CACHED_BLOCKS: usize = 32;

/// Wrapped AES-256 key plus GCM tag.
pub const WRAPPED_KEY_SIZE: usize = crate::crypto::MASTER_KEY_SIZE + TAG_SIZE;
pub const UUID_SIZE: usize = 16;
pub const RESERVED_SIZE: usize = 92;

// Offsets
const OFF_MAGIC: usize = 0;
const OFF_VERSION: usize = 16;
const OFF_FLAGS: usize = 18;
const OFF_SALT: usize = 20;
const OFF_NONCE: usize = 52;
const OFF_TAG: usize = 64;
const OFF_WRAPPED_KEY: usize = 80;
const OFF_CONTAINER_ID: usize = 128;
const OFF_KDF_ALGORITHM: usize = 144;
const OFF_KDF_SCHEMA_VERSION: usize = 146;
const OFF_KDF_MEMORY_KIB: usize = 148;
const OFF_KDF_ITERATIONS: usize = 152;
const OFF_KDF_PARALLELISM: usize = 156;
const OFF_KDF_OUTPUT_LEN: usize = 160;
const OFF_RESERVED: usize = 164;

/// Container header — always exactly HEADER_SIZE bytes.
///
/// `tag` is reserved for future use (e.g. authenticated additional data).
/// Currently zero-filled on creation, ignored on read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerHeader {
    pub magic: [u8; 16],
    pub version: u16,
    pub flags: ContainerFlags,
    pub salt: [u8; SALT_SIZE],
    pub nonce: [u8; NONCE_SIZE],
    pub tag: [u8; TAG_SIZE],
    pub wrapped_master_key: [u8; WRAPPED_KEY_SIZE],
    pub container_id: Uuid,
    pub kdf_config: KdfConfig,
}

impl ContainerHeader {
    /// Create a new header.
    pub fn new(
        flags: ContainerFlags,
        salt: [u8; SALT_SIZE],
        nonce: [u8; NONCE_SIZE],
        tag: [u8; TAG_SIZE],
        wrapped_master_key: [u8; WRAPPED_KEY_SIZE],
        kdf_config: KdfConfig,
    ) -> Result<Self> {
        kdf_config.validate()?;
        Ok(ContainerHeader {
            magic: *MAGIC,
            version: CURRENT_VERSION,
            flags,
            salt,
            nonce,
            tag,
            wrapped_master_key,
            container_id: Uuid::new_v4(),
            kdf_config,
        })
    }

    /// Serialize to exactly HEADER_SIZE bytes.
    pub fn to_bytes(&self) -> Result<[u8; HEADER_SIZE]> {
        if self.version != CURRENT_VERSION {
            return Err(SesameError::UnsupportedVersion {
                found: self.version,
                expected: vec![CURRENT_VERSION],
                guidance: "recreate the container with this release".into(),
            });
        }
        self.kdf_config.validate()?;
        let mut buf = [0u8; HEADER_SIZE];
        buf[OFF_MAGIC..OFF_MAGIC + 16].copy_from_slice(&self.magic);
        buf[OFF_VERSION..OFF_VERSION + 2].copy_from_slice(&self.version.to_be_bytes());
        buf[OFF_FLAGS..OFF_FLAGS + 2].copy_from_slice(&self.flags.to_u16().to_be_bytes());
        buf[OFF_SALT..OFF_SALT + SALT_SIZE].copy_from_slice(&self.salt);
        buf[OFF_NONCE..OFF_NONCE + NONCE_SIZE].copy_from_slice(&self.nonce);
        buf[OFF_TAG..OFF_TAG + TAG_SIZE].copy_from_slice(&self.tag);
        buf[OFF_WRAPPED_KEY..OFF_WRAPPED_KEY + WRAPPED_KEY_SIZE]
            .copy_from_slice(&self.wrapped_master_key);
        buf[OFF_CONTAINER_ID..OFF_CONTAINER_ID + UUID_SIZE]
            .copy_from_slice(self.container_id.as_bytes());
        buf[OFF_KDF_ALGORITHM..OFF_KDF_ALGORITHM + 2]
            .copy_from_slice(&self.kdf_config.algorithm.code().to_be_bytes());
        buf[OFF_KDF_SCHEMA_VERSION..OFF_KDF_SCHEMA_VERSION + 2]
            .copy_from_slice(&self.kdf_config.schema_version.to_be_bytes());
        buf[OFF_KDF_MEMORY_KIB..OFF_KDF_MEMORY_KIB + 4]
            .copy_from_slice(&self.kdf_config.memory_kib.to_be_bytes());
        buf[OFF_KDF_ITERATIONS..OFF_KDF_ITERATIONS + 4]
            .copy_from_slice(&self.kdf_config.iterations.to_be_bytes());
        buf[OFF_KDF_PARALLELISM..OFF_KDF_PARALLELISM + 4]
            .copy_from_slice(&self.kdf_config.parallelism.to_be_bytes());
        buf[OFF_KDF_OUTPUT_LEN..OFF_KDF_OUTPUT_LEN + 4]
            .copy_from_slice(&self.kdf_config.output_len.to_be_bytes());
        // reserved stays zero
        Ok(buf)
    }

    /// Deserialize from exactly HEADER_SIZE bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER_SIZE {
            return Err(SesameError::Corruption(
                "header is shorter than 256 bytes".into(),
            ));
        }

        let mut magic = [0u8; 16];
        magic.copy_from_slice(&bytes[OFF_MAGIC..OFF_MAGIC + 16]);
        if magic != *MAGIC {
            return Err(SesameError::NotASesameContainer);
        }

        let version = u16::from_be_bytes([bytes[OFF_VERSION], bytes[OFF_VERSION + 1]]);
        if version != CURRENT_VERSION {
            let migration = if version == 2 {
                "V2 containers are no longer supported; recreate the container with this release"
            } else {
                "open the container with a release that supports this format"
            };
            return Err(SesameError::UnsupportedVersion {
                found: version,
                expected: vec![CURRENT_VERSION],
                guidance: migration.into(),
            });
        }
        let flags_raw = u16::from_be_bytes([bytes[OFF_FLAGS], bytes[OFF_FLAGS + 1]]);

        let mut salt = [0u8; SALT_SIZE];
        salt.copy_from_slice(&bytes[OFF_SALT..OFF_SALT + SALT_SIZE]);

        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&bytes[OFF_NONCE..OFF_NONCE + NONCE_SIZE]);

        let mut tag = [0u8; TAG_SIZE];
        tag.copy_from_slice(&bytes[OFF_TAG..OFF_TAG + TAG_SIZE]);

        let mut wrapped_master_key = [0u8; WRAPPED_KEY_SIZE];
        wrapped_master_key
            .copy_from_slice(&bytes[OFF_WRAPPED_KEY..OFF_WRAPPED_KEY + WRAPPED_KEY_SIZE]);

        let mut container_id_bytes = [0u8; UUID_SIZE];
        container_id_bytes.copy_from_slice(&bytes[OFF_CONTAINER_ID..OFF_CONTAINER_ID + UUID_SIZE]);
        let container_id = Uuid::from_bytes(container_id_bytes);

        let algorithm_code =
            u16::from_be_bytes([bytes[OFF_KDF_ALGORITHM], bytes[OFF_KDF_ALGORITHM + 1]]);
        let kdf_config = KdfConfig {
            algorithm: KdfAlgorithm::from_code(algorithm_code).map_err(|error| {
                SesameError::Corruption(format!("invalid KDF configuration: {error}"))
            })?,
            schema_version: u16::from_be_bytes([
                bytes[OFF_KDF_SCHEMA_VERSION],
                bytes[OFF_KDF_SCHEMA_VERSION + 1],
            ]),
            memory_kib: read_u32(bytes, OFF_KDF_MEMORY_KIB),
            iterations: read_u32(bytes, OFF_KDF_ITERATIONS),
            parallelism: read_u32(bytes, OFF_KDF_PARALLELISM),
            output_len: read_u32(bytes, OFF_KDF_OUTPUT_LEN),
        };
        kdf_config.validate().map_err(|error| {
            SesameError::Corruption(format!("invalid KDF configuration: {error}"))
        })?;
        if bytes[OFF_RESERVED..HEADER_SIZE]
            .iter()
            .any(|byte| *byte != 0)
        {
            return Err(SesameError::Corruption(
                "V3 header reserved bytes must be zero".into(),
            ));
        }

        Ok(ContainerHeader {
            magic,
            version,
            flags: ContainerFlags::from_u16(flags_raw),
            salt,
            nonce,
            tag,
            wrapped_master_key,
            container_id,
            kdf_config,
        })
    }

    pub fn verify_magic(&self) -> bool {
        self.magic == *MAGIC
    }

    pub fn is_supported_version(&self) -> bool {
        self.version == CURRENT_VERSION
    }

    pub fn flags_u16(&self) -> u16 {
        self.flags.to_u16()
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// Derive a 12-byte nonce for a specific page.
/// Nonce = big-endian encoding of global page number (including metadata blocks),
///          zero-padded to 12 bytes.
///
/// Data pages start at block 2 (after file table + bitmap), so page 0 in a data block
/// corresponds to global_block = data_block_index + 2.
pub fn page_nonce(data_block_index: u64, page_in_block: u64) -> [u8; NONCE_SIZE] {
    let global_page = (2 + data_block_index) * PAGES_PER_BLOCK as u64 + page_in_block;
    let bytes = global_page.to_be_bytes();
    let mut nonce = [0u8; NONCE_SIZE];
    nonce[4..12].copy_from_slice(&bytes);
    nonce
}
