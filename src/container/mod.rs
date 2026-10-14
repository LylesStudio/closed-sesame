mod checksum;
pub mod defrag;
pub mod format;
mod lock;
pub mod metadata;
pub mod path;
pub mod reader;
mod storage;
pub mod writer;

#[cfg(feature = "gui")]
pub(crate) use checksum::crc32;
pub use checksum::verify_crc32;
pub use format::ContainerHeader;
pub use lock::VaultLock;
pub use metadata::{AllocationBitmap, ContainerFlags, FileEntry};
pub use path::{join, normalize, parent};
pub use reader::ContainerReader;
pub use writer::{ContainerWriter, validate_entry_tree};

/// Temporary source-compatibility façade. New code should import from `container`.
pub mod io {
    pub use super::{ContainerReader, ContainerWriter};
}
