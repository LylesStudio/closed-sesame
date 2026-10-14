//! ClosedSesame encrypted-container library.
//!
//! The crate reads and writes the V3 `.sesame` format. Frontends should call
//! [`app`] use cases instead of duplicating credential or container orchestration.
//!
//! User-visible strings are localized via [`i18n`] and `locales/*.yml`.

rust_i18n::i18n!("locales", fallback = "en");

/// Like [`rust_i18n::t!`] but returns an owned [`String`].
#[macro_export]
macro_rules! ts {
    ($($args:tt)*) => {{
        rust_i18n::t!($($args)*).into_owned()
    }};
}

pub mod app;
pub mod container;
pub mod crypto;
pub mod error;
#[cfg(feature = "gui")]
pub mod gui;
pub mod i18n;
pub mod keymgmt;
#[cfg(feature = "mount")]
pub mod mount;
pub mod mount_hints;

pub use container::{ContainerReader, ContainerWriter, FileEntry};
pub use error::{Result, SesameError};
