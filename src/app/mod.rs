//! Platform- and frontend-independent application use cases.

pub mod credentials;
pub mod recovery;
pub mod size;
pub mod symlink;
pub mod vault;

pub use credentials::{Credential, prepare_credentials, read_header};
pub use size::{ensure_sesame_extension, fmt_size, parse_size};
