use crate::i18n::localized_invalid_input;
use thiserror::Error;

/// All errors that ClosedSesame can produce.
/// All I/O paths return Result<T, SesameError> — no unwrap() in production code.
#[derive(Error, Debug)]
pub enum SesameError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("cryptographic error: {0}")]
    Crypto(String),

    #[error("container corrupted: {0}")]
    Corruption(String),

    #[error("wrong password")]
    WrongPassword,

    #[error("unsupported container version {found} (expected one of: {expected:?}); {guidance}")]
    UnsupportedVersion {
        found: u16,
        expected: Vec<u16>,
        guidance: String,
    },

    #[error("not a ClosedSesame container (magic bytes mismatch)")]
    NotASesameContainer,

    #[error("disk full: needed {needed} bytes, only {available} available")]
    DiskFull { needed: u64, available: u64 },

    #[error("file busy: {0} files still open")]
    FileBusy(usize),

    #[error("container is in use by another process: {0}")]
    ContainerBusy(String),

    #[error("container already mounted at {0}")]
    AlreadyMounted(String),

    #[error("container not mounted")]
    NotMounted,

    #[error("platform error: {0}")]
    Platform(String),

    #[error("system clock error: {0}")]
    SystemClock(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("extract destination conflict: {0}")]
    ExtractConflict(String),

    #[error("extract destination is inside a mounted vault: {0}")]
    ExtractMountReadOnly(String),
}

impl SesameError {
    pub fn localized(&self) -> String {
        match self {
            Self::Io(error) => ts!("error.io", detail = error.to_string()),
            Self::Crypto(message) => ts!("error.crypto", detail = message),
            Self::Corruption(message) => ts!("error.corruption", detail = message),
            Self::WrongPassword => ts!("error.wrong_password"),
            Self::UnsupportedVersion {
                found,
                expected,
                guidance,
            } => ts!(
                "error.unsupported_version",
                found = *found,
                expected = format!("{expected:?}"),
                guidance = guidance
            ),
            Self::NotASesameContainer => ts!("error.not_a_sesame_container"),
            Self::DiskFull { needed, available } => {
                ts!("error.disk_full", needed = *needed, available = *available)
            }
            Self::FileBusy(count) => ts!("error.file_busy", count = *count),
            Self::ContainerBusy(path) => ts!("error.container_busy", path = path),
            Self::AlreadyMounted(path) => ts!("error.already_mounted", path = path),
            Self::NotMounted => ts!("error.not_mounted"),
            Self::Platform(message) => ts!("error.platform", detail = message),
            Self::SystemClock(message) => ts!("error.system_clock", detail = message),
            Self::Serialization(message) => ts!("error.serialization", detail = message),
            Self::InvalidInput(message) => localized_invalid_input(message),
            Self::ExtractConflict(path) => ts!("error.extract.destination_conflict", path = path),
            Self::ExtractMountReadOnly(path) => ts!("error.extract.mount_read_only", path = path),
        }
    }
}

pub type Result<T> = std::result::Result<T, SesameError>;
