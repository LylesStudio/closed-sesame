//! Cryptographic primitives for ClosedSesame.
//!
//! All cryptographic operations are centralized here. No other module
//! calls encryption/decryption/derivation APIs directly.
//!
//! Sources:
//! - AES-256-GCM: https://docs.rs/aes-gcm/0.10
//! - Argon2id: https://docs.rs/argon2/0.5
//! - HKDF-SHA256: https://docs.rs/hkdf/0.12

use crate::error::{Result, SesameError};
use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, OsRng},
};
use argon2::{Argon2, ParamsBuilder};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use zeroize::Zeroize;

// ——— Constants ———

/// AES-GCM nonce size (12 bytes, per NIST SP 800-38D §8.2)
pub const NONCE_SIZE: usize = 12;
/// AES-256 key size
pub const KEY_SIZE: usize = 32;
/// AES-GCM authentication tag size
pub const TAG_SIZE: usize = 16;
/// Argon2id salt size
pub const SALT_SIZE: usize = 32;
/// Master key size for container
pub const MASTER_KEY_SIZE: usize = 32;

/// On-disk KDF configuration schema understood by this release.
pub const KDF_SCHEMA_VERSION: u16 = 1;
pub const DEFAULT_ARGON2_MEMORY_KIB: u32 = 64 * 1024;
pub const DEFAULT_ARGON2_ITERATIONS: u32 = 3;
pub const DEFAULT_ARGON2_PARALLELISM: u32 = 1;
const MAX_ARGON2_MEMORY_KIB: u32 = 256 * 1024;
const MAX_ARGON2_ITERATIONS: u32 = 10;
const MAX_ARGON2_PARALLELISM: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum KdfAlgorithm {
    Argon2id = 1,
    NoPassword = 2,
}

impl KdfAlgorithm {
    pub fn from_code(code: u16) -> Result<Self> {
        match code {
            1 => Ok(Self::Argon2id),
            2 => Ok(Self::NoPassword),
            _ => Err(SesameError::InvalidInput(format!(
                "unknown KDF algorithm {code}; recreate the container with a supported release"
            ))),
        }
    }

    pub const fn code(self) -> u16 {
        self as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfConfig {
    pub algorithm: KdfAlgorithm,
    pub schema_version: u16,
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub output_len: u32,
}

impl KdfConfig {
    pub const fn default_password() -> Self {
        Self {
            algorithm: KdfAlgorithm::Argon2id,
            schema_version: KDF_SCHEMA_VERSION,
            memory_kib: DEFAULT_ARGON2_MEMORY_KIB,
            iterations: DEFAULT_ARGON2_ITERATIONS,
            parallelism: DEFAULT_ARGON2_PARALLELISM,
            output_len: KEY_SIZE as u32,
        }
    }

    pub const fn no_password() -> Self {
        Self {
            algorithm: KdfAlgorithm::NoPassword,
            schema_version: KDF_SCHEMA_VERSION,
            memory_kib: 0,
            iterations: 0,
            parallelism: 0,
            output_len: KEY_SIZE as u32,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != KDF_SCHEMA_VERSION {
            return Err(SesameError::InvalidInput(format!(
                "unsupported KDF schema version {}; supported version is {}",
                self.schema_version, KDF_SCHEMA_VERSION
            )));
        }
        if self.output_len != KEY_SIZE as u32 {
            return Err(SesameError::InvalidInput(format!(
                "KDF output_len must be {KEY_SIZE}, got {}",
                self.output_len
            )));
        }
        match self.algorithm {
            KdfAlgorithm::Argon2id => {
                if self.memory_kib == 0 || self.memory_kib > MAX_ARGON2_MEMORY_KIB {
                    return Err(SesameError::InvalidInput(format!(
                        "Argon2 memory_kib must be between 1 and {MAX_ARGON2_MEMORY_KIB}"
                    )));
                }
                if self.iterations == 0 || self.iterations > MAX_ARGON2_ITERATIONS {
                    return Err(SesameError::InvalidInput(format!(
                        "Argon2 iterations must be between 1 and {MAX_ARGON2_ITERATIONS}"
                    )));
                }
                if self.parallelism == 0 || self.parallelism > MAX_ARGON2_PARALLELISM {
                    return Err(SesameError::InvalidInput(format!(
                        "Argon2 parallelism must be between 1 and {MAX_ARGON2_PARALLELISM}"
                    )));
                }
                if self.memory_kib < self.parallelism.saturating_mul(8) {
                    return Err(SesameError::InvalidInput(
                        "Argon2 memory_kib must be at least 8 times parallelism".into(),
                    ));
                }
            }
            KdfAlgorithm::NoPassword => {
                if self.memory_kib != 0 || self.iterations != 0 || self.parallelism != 0 {
                    return Err(SesameError::InvalidInput(
                        "no-password KDF cost fields must all be zero".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

// ——— Key Types ———

/// A cryptographic key that is zeroed on drop.
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct Key([u8; KEY_SIZE]);

impl Key {
    pub fn from_slice(slice: &[u8]) -> Result<Self> {
        if slice.len() != KEY_SIZE {
            return Err(SesameError::Crypto(format!(
                "key must be {} bytes, got {}",
                KEY_SIZE,
                slice.len()
            )));
        }
        let mut key = [0u8; KEY_SIZE];
        key.copy_from_slice(slice);
        Ok(Key(key))
    }

    pub fn as_bytes(&self) -> &[u8; KEY_SIZE] {
        &self.0
    }
}

/// A randomly generated salt value.
pub struct Salt([u8; SALT_SIZE]);

impl Salt {
    pub fn generate() -> Self {
        let mut salt = [0u8; SALT_SIZE];
        OsRng.fill_bytes(&mut salt);
        Salt(salt)
    }

    pub fn from_slice(slice: &[u8]) -> Result<Self> {
        if slice.len() != SALT_SIZE {
            return Err(SesameError::Crypto(format!(
                "salt must be {} bytes, got {}",
                SALT_SIZE,
                slice.len()
            )));
        }
        let mut salt = [0u8; SALT_SIZE];
        salt.copy_from_slice(slice);
        Ok(Salt(salt))
    }

    pub fn as_bytes(&self) -> &[u8; SALT_SIZE] {
        &self.0
    }
}

// ——— Core Operations ———

/// Derive a key using the self-described KDF configuration.
pub fn derive_key(password: &str, salt: &Salt, config: &KdfConfig) -> Result<Key> {
    config.validate()?;
    if config.algorithm != KdfAlgorithm::Argon2id {
        return Err(SesameError::InvalidInput(
            "password credentials require the Argon2id KDF algorithm".into(),
        ));
    }
    let params = ParamsBuilder::new()
        .m_cost(config.memory_kib)
        .t_cost(config.iterations)
        .p_cost(config.parallelism)
        .output_len(config.output_len as usize)
        .build()
        .map_err(|e| SesameError::Crypto(format!("Argon2 params error: {e}")))?;

    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);

    let mut output = [0u8; KEY_SIZE];
    argon2
        .hash_password_into(password.as_bytes(), salt.as_bytes(), &mut output)
        .map_err(|e| SesameError::Crypto(format!("Argon2 hash error: {e}")))?;

    Ok(Key(output))
}

/// Expand a key into derived keys using HKDF-SHA256.
///
/// Each container gets a unique master key derived via:
///   MK = HKDF-SHA256(KEK, salt="sesame-container-key", info=container_id)
///
/// Source: https://docs.rs/hkdf/0.12/hkdf/struct.Hkdf.html
pub fn expand_key(kek: &Key, info: &[u8]) -> Result<Key> {
    let hkdf = Hkdf::<Sha256>::new(Some(b"sesame-container-key"), kek.as_bytes());
    let mut output = [0u8; KEY_SIZE];
    hkdf.expand(info, &mut output)
        .map_err(|e| SesameError::Crypto(format!("HKDF expand error: {e}")))?;
    Ok(Key(output))
}

/// Generate a cryptographically random nonce for AES-GCM.
pub fn generate_nonce() -> [u8; NONCE_SIZE] {
    let mut nonce = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce);
    nonce
}

/// Encrypt with AES-256-GCM using a random nonce.
/// Returns (ciphertext+tag, nonce). For full-block encryption.
pub fn encrypt(key: &Key, plaintext: &[u8]) -> Result<(Vec<u8>, [u8; NONCE_SIZE])> {
    let nonce = generate_nonce();
    let ct = encrypt_with_nonce(key, plaintext, &nonce)?;
    Ok((ct, nonce))
}

/// Encrypt plaintext with AES-256-GCM using a provided nonce.
/// Used for deterministic page-level encryption where nonce = page index.
pub fn encrypt_with_nonce(
    key: &Key,
    plaintext: &[u8],
    nonce: &[u8; NONCE_SIZE],
) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
        .map_err(|e| SesameError::Crypto(format!("AES init error: {e}")))?;
    let n = Nonce::from_slice(nonce);
    cipher
        .encrypt(n, plaintext)
        .map_err(|e| SesameError::Crypto(format!("AES-GCM encrypt error: {e}")))
}

/// Decrypt ciphertext with AES-256-GCM.
///
/// Verifies the authentication tag BEFORE returning any plaintext.
/// Returns error on authentication failure (wrong key, corrupted data, tampering).
pub fn decrypt(key: &Key, ciphertext: &[u8], nonce: &[u8; NONCE_SIZE]) -> Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key.as_bytes())
        .map_err(|e| SesameError::Crypto(format!("AES init error: {e}")))?;

    let nonce = Nonce::from_slice(nonce);

    cipher.decrypt(nonce, ciphertext).map_err(|_| {
        SesameError::Crypto("AES-GCM authentication failed — wrong key or corrupted data".into())
    })
}

/// Generate random bytes for key material. Caller must zeroize.
pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    OsRng.fill_bytes(&mut buf);
    buf
}

/// Generate random key material directly into a Key (no intermediate Vec).
pub fn random_key() -> Key {
    let mut bytes = [0u8; KEY_SIZE];
    OsRng.fill_bytes(&mut bytes);
    Key(bytes)
}

// ——— Key Wrapping ———

/// Wrap (encrypt) a master key with a key-encrypting key using AES-256-GCM.
///
/// Returns (wrapped_key, nonce) — store both in the container header.
pub fn wrap_key(kek: &Key, mk: &Key) -> Result<(Vec<u8>, [u8; NONCE_SIZE])> {
    encrypt(kek, mk.as_bytes())
}

/// Unwrap (decrypt) a master key with a key-encrypting key.
pub fn unwrap_key(kek: &Key, wrapped: &[u8], nonce: &[u8; NONCE_SIZE]) -> Result<Key> {
    let plaintext = decrypt(kek, wrapped, nonce)?;
    Key::from_slice(&plaintext)
}

// ——— Deterministic KDF (no-password mode) ———

/// Derive a KEK for no-password mode.
///
/// KEK = HKDF-SHA256(ikm="closed-sesame-no-password", salt=header.salt, info="kek")
///
/// This is NOT cryptographically secure against a human attacker —
/// anyone who knows this algorithm can derive the key. It is designed
/// to stop AI agents from accidentally reading encrypted data by
/// requiring knowledge of the derivation protocol.
pub fn deterministic_kdf(salt: &Salt, config: &KdfConfig) -> Result<Key> {
    config.validate()?;
    if config.algorithm != KdfAlgorithm::NoPassword {
        return Err(SesameError::InvalidInput(
            "no-password credentials require the no-password KDF algorithm".into(),
        ));
    }
    let hkdf = Hkdf::<Sha256>::new(Some(salt.as_bytes()), b"closed-sesame-no-password");
    let mut output = [0u8; KEY_SIZE];
    hkdf.expand(b"sesame-kek-no-password", &mut output)
        .map_err(|error| SesameError::Crypto(format!("HKDF expand error: {error}")))?;
    Ok(Key(output))
}
