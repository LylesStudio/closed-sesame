//! Key management for ClosedSesame containers.
//!
//! Two modes:
//! - **Password-protected**: Argon2id(password, salt) → KEK
//! - **No-password**: HKDF("closed-sesame-no-password", salt) → KEK
//!
//! Creation generates a random per-container master key (MK), wraps it with
//! the KEK, and stores the wrapped key in the redundant container headers.

use crate::crypto::{self, KdfAlgorithm, KdfConfig, Key, NONCE_SIZE, Salt};
use crate::error::Result;

/// Key management mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyMode {
    PasswordProtected,
    NoPassword,
}

/// A key-encrypting key, derived from user credentials or deterministically.
pub struct KeyEncryptingKey {
    key: Key,
    config: KdfConfig,
}

impl Clone for KeyEncryptingKey {
    fn clone(&self) -> Self {
        // Re-wrap key bytes instead of deriving `Clone` on the zeroizing
        // `Key` type; the copy is still zeroized on drop.
        Self {
            key: Key::from_slice(self.key.as_bytes()).expect("key length is fixed"),
            config: self.config,
        }
    }
}

impl KeyEncryptingKey {
    /// Derive a KEK from a password using Argon2id.
    pub fn from_password(password: &str, salt: &Salt, config: KdfConfig) -> Result<Self> {
        let key = crypto::derive_key(password, salt, &config)?;
        Ok(KeyEncryptingKey { key, config })
    }

    /// Derive a KEK deterministically (no-password mode).
    ///
    /// This is intentionally not cryptographically strong against a human —
    /// anyone who knows the algorithm can reproduce the key. It exists to
    /// prevent AI agents from accidentally reading the data.
    pub fn no_password(salt: &Salt, config: KdfConfig) -> Result<Self> {
        let key = crypto::deterministic_kdf(salt, &config)?;
        Ok(KeyEncryptingKey { key, config })
    }

    pub fn mode(&self) -> KeyMode {
        match self.config.algorithm {
            KdfAlgorithm::Argon2id => KeyMode::PasswordProtected,
            KdfAlgorithm::NoPassword => KeyMode::NoPassword,
        }
    }

    pub fn config(&self) -> KdfConfig {
        self.config
    }

    /// Wrap (encrypt) a master key for storage in the container header.
    pub fn wrap_master_key(&self, mk: &Key) -> Result<(Vec<u8>, [u8; NONCE_SIZE])> {
        crypto::wrap_key(&self.key, mk)
    }

    /// Unwrap (decrypt) a master key from the container header.
    pub fn unwrap_master_key(&self, wrapped: &[u8], nonce: &[u8; NONCE_SIZE]) -> Result<Key> {
        crypto::unwrap_key(&self.key, wrapped, nonce)
    }
}
