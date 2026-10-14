//! Integration tests for ClosedSesame cryptographic primitives.
//!
//! These tests verify the core security properties:
//! - AES-256-GCM encrypt/decrypt roundtrip
//! - Authentication failure on wrong key / corrupted data
//! - Argon2id key derivation determinism
//! - HKDF key expansion
//! - Key wrapping/unwrapping
//! - No-password mode deterministic KDF

use closed_sesame::container::format::{
    BLOCK_SIZE, CURRENT_VERSION, ContainerFlags, ContainerHeader, FileEntry,
    NONCE_SIZE as _HDR_NONCE, SALT_SIZE, TAG_SIZE, WRAPPED_KEY_SIZE,
};
use closed_sesame::container::metadata::{
    AllocationBitmap, decode_bitmap, decode_file_table, encode_bitmap, encode_file_table,
};
use closed_sesame::crypto::{self, KdfAlgorithm, KdfConfig, Key, Salt};

// ——— AES-256-GCM roundtrip ———

#[test]
fn aes_gcm_roundtrip_empty() {
    let key = Key::from_slice(&[42u8; 32]).unwrap();
    let plaintext = b"";
    let (ciphertext, nonce) = crypto::encrypt(&key, plaintext).unwrap();
    let decrypted = crypto::decrypt(&key, &ciphertext, &nonce).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn aes_gcm_roundtrip_small() {
    let key = Key::from_slice(&[42u8; 32]).unwrap();
    let plaintext = b"hello world";
    let (ciphertext, nonce) = crypto::encrypt(&key, plaintext).unwrap();
    let decrypted = crypto::decrypt(&key, &ciphertext, &nonce).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn aes_gcm_roundtrip_4mb() {
    let key = Key::from_slice(&[42u8; 32]).unwrap();
    let plaintext = vec![0xAB; BLOCK_SIZE];
    let (ciphertext, nonce) = crypto::encrypt(&key, &plaintext).unwrap();
    let decrypted = crypto::decrypt(&key, &ciphertext, &nonce).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn aes_gcm_wrong_key_fails() {
    let key1 = Key::from_slice(&[1u8; 32]).unwrap();
    let key2 = Key::from_slice(&[2u8; 32]).unwrap();
    let plaintext = b"secret";
    let (ciphertext, nonce) = crypto::encrypt(&key1, plaintext).unwrap();
    assert!(crypto::decrypt(&key2, &ciphertext, &nonce).is_err());
}

#[test]
fn aes_gcm_corrupted_ciphertext_fails() {
    let key = Key::from_slice(&[42u8; 32]).unwrap();
    let plaintext = b"corrupt me";
    let (mut ciphertext, nonce) = crypto::encrypt(&key, plaintext).unwrap();
    // Flip a bit in the ciphertext
    ciphertext[0] ^= 1;
    assert!(crypto::decrypt(&key, &ciphertext, &nonce).is_err());
}

#[test]
fn aes_gcm_wrong_nonce_fails() {
    let key = Key::from_slice(&[42u8; 32]).unwrap();
    let plaintext = b"wrong nonce";
    let (ciphertext, nonce) = crypto::encrypt(&key, plaintext).unwrap();
    let mut wrong_nonce = nonce;
    wrong_nonce[0] ^= 1;
    assert!(crypto::decrypt(&key, &ciphertext, &wrong_nonce).is_err());
}

#[test]
fn aes_gcm_non_deterministic() {
    let key = Key::from_slice(&[42u8; 32]).unwrap();
    let plaintext = b"test";
    let (ct1, _) = crypto::encrypt(&key, plaintext).unwrap();
    let (ct2, _) = crypto::encrypt(&key, plaintext).unwrap();
    // Same plaintext should produce different ciphertexts due to random nonce
    assert_ne!(ct1, ct2);
}

// ——— Argon2id key derivation ———

#[test]
fn argon2id_deterministic() {
    let salt = Salt::from_slice(&[7u8; 32]).unwrap();
    let config = KdfConfig::default_password();
    let key1 = crypto::derive_key("password123", &salt, &config).unwrap();
    let key2 = crypto::derive_key("password123", &salt, &config).unwrap();
    assert_eq!(key1.as_bytes(), key2.as_bytes());
}

#[test]
fn argon2id_different_password_different_key() {
    let salt = Salt::from_slice(&[7u8; 32]).unwrap();
    let config = KdfConfig::default_password();
    let key1 = crypto::derive_key("password123", &salt, &config).unwrap();
    let key2 = crypto::derive_key("password124", &salt, &config).unwrap();
    assert_ne!(key1.as_bytes(), key2.as_bytes());
}

#[test]
fn argon2id_different_salt_different_key() {
    let salt1 = Salt::from_slice(&[1u8; 32]).unwrap();
    let salt2 = Salt::from_slice(&[2u8; 32]).unwrap();
    let config = KdfConfig::default_password();
    let key1 = crypto::derive_key("password123", &salt1, &config).unwrap();
    let key2 = crypto::derive_key("password123", &salt2, &config).unwrap();
    assert_ne!(key1.as_bytes(), key2.as_bytes());
}

// ——— HKDF ———

#[test]
fn hkdf_expand_deterministic() {
    let kek = Key::from_slice(&[42u8; 32]).unwrap();
    let mk1 = crypto::expand_key(&kek, b"container-1").unwrap();
    let mk2 = crypto::expand_key(&kek, b"container-1").unwrap();
    assert_eq!(mk1.as_bytes(), mk2.as_bytes());
}

#[test]
fn hkdf_different_info_different_key() {
    let kek = Key::from_slice(&[42u8; 32]).unwrap();
    let mk1 = crypto::expand_key(&kek, b"container-1").unwrap();
    let mk2 = crypto::expand_key(&kek, b"container-2").unwrap();
    assert_ne!(mk1.as_bytes(), mk2.as_bytes());
}

// ——— Key wrapping ———

#[test]
fn key_wrap_unwrap_roundtrip() {
    let kek = Key::from_slice(&[1u8; 32]).unwrap();
    let mk = Key::from_slice(&[2u8; 32]).unwrap();
    let (wrapped, nonce) = crypto::wrap_key(&kek, &mk).unwrap();
    let unwrapped = crypto::unwrap_key(&kek, &wrapped, &nonce).unwrap();
    assert_eq!(mk.as_bytes(), unwrapped.as_bytes());
}

#[test]
fn key_unwrap_wrong_kek_fails() {
    let kek1 = Key::from_slice(&[1u8; 32]).unwrap();
    let kek2 = Key::from_slice(&[2u8; 32]).unwrap();
    let mk = Key::from_slice(&[3u8; 32]).unwrap();
    let (wrapped, nonce) = crypto::wrap_key(&kek1, &mk).unwrap();
    assert!(crypto::unwrap_key(&kek2, &wrapped, &nonce).is_err());
}

// ——— No-password mode ———

#[test]
fn deterministic_kdf_produces_same_key() {
    let salt = Salt::from_slice(&[7u8; 32]).unwrap();
    let config = KdfConfig::no_password();
    let key1 = crypto::deterministic_kdf(&salt, &config).unwrap();
    let key2 = crypto::deterministic_kdf(&salt, &config).unwrap();
    assert_eq!(key1.as_bytes(), key2.as_bytes());
}

#[test]
fn deterministic_kdf_different_salt_different_key() {
    let salt1 = Salt::from_slice(&[1u8; 32]).unwrap();
    let salt2 = Salt::from_slice(&[2u8; 32]).unwrap();
    let config = KdfConfig::no_password();
    let key1 = crypto::deterministic_kdf(&salt1, &config).unwrap();
    let key2 = crypto::deterministic_kdf(&salt2, &config).unwrap();
    assert_ne!(key1.as_bytes(), key2.as_bytes());
}

// ——— Container header ———

#[test]
fn header_new_has_correct_magic() {
    let header = ContainerHeader::new(
        ContainerFlags::NONE,
        [0u8; SALT_SIZE],
        [0u8; _HDR_NONCE],
        [0u8; TAG_SIZE],
        [0u8; WRAPPED_KEY_SIZE],
        KdfConfig::default_password(),
    )
    .unwrap();
    assert!(header.verify_magic());
    assert_eq!(header.version, CURRENT_VERSION);
}

#[test]
fn header_roundtrip_serialization() {
    let header = ContainerHeader::new(
        ContainerFlags::NONE,
        [7u8; SALT_SIZE],
        [3u8; _HDR_NONCE],
        [9u8; TAG_SIZE],
        [42u8; WRAPPED_KEY_SIZE],
        KdfConfig::default_password(),
    )
    .unwrap();
    let bytes = header.to_bytes().unwrap();
    let deserialized = ContainerHeader::from_bytes(&bytes).unwrap();
    assert_eq!(header.magic, deserialized.magic);
    assert_eq!(header.version, deserialized.version);
    assert_eq!(header.salt, deserialized.salt);
    assert_eq!(header.container_id, deserialized.container_id);
    assert_eq!(header.kdf_config, deserialized.kdf_config);
}

#[test]
fn header_rejects_wrong_magic() {
    let header = ContainerHeader::new(
        ContainerFlags::NONE,
        [0u8; SALT_SIZE],
        [0u8; _HDR_NONCE],
        [0u8; TAG_SIZE],
        [0u8; WRAPPED_KEY_SIZE],
        KdfConfig::default_password(),
    )
    .unwrap();
    assert!(header.verify_magic());
}

#[test]
fn header_version_supported() {
    let header = ContainerHeader::new(
        ContainerFlags::NONE,
        [0u8; SALT_SIZE],
        [0u8; _HDR_NONCE],
        [0u8; TAG_SIZE],
        [0u8; WRAPPED_KEY_SIZE],
        KdfConfig::default_password(),
    )
    .unwrap();
    assert!(header.is_supported_version());
}

#[test]
fn v3_header_roundtrips_every_kdf_field_big_endian() {
    let config = KdfConfig {
        algorithm: KdfAlgorithm::Argon2id,
        schema_version: 1,
        memory_kib: 0x0001_0203,
        iterations: 3,
        parallelism: 2,
        output_len: 32,
    };
    let header = ContainerHeader::new(
        ContainerFlags::NONE,
        [1; SALT_SIZE],
        [2; _HDR_NONCE],
        [0; TAG_SIZE],
        [3; WRAPPED_KEY_SIZE],
        config,
    )
    .unwrap();
    let bytes = header.to_bytes().unwrap();
    assert_eq!(&bytes[144..146], &1u16.to_be_bytes());
    assert_eq!(&bytes[146..148], &1u16.to_be_bytes());
    assert_eq!(&bytes[148..152], &config.memory_kib.to_be_bytes());
    assert_eq!(&bytes[152..156], &config.iterations.to_be_bytes());
    assert_eq!(&bytes[156..160], &config.parallelism.to_be_bytes());
    assert_eq!(&bytes[160..164], &config.output_len.to_be_bytes());
    assert!(bytes[164..].iter().all(|byte| *byte == 0));
    assert_eq!(
        ContainerHeader::from_bytes(&bytes).unwrap().kdf_config,
        config
    );
}

#[test]
fn v2_header_is_explicitly_rejected() {
    let mut bytes = [0u8; 256];
    bytes[..16].copy_from_slice(b"SESAMEBOX\0\0\0\0\0\0\0");
    bytes[16..18].copy_from_slice(&2u16.to_be_bytes());
    let error = ContainerHeader::from_bytes(&bytes).unwrap_err();
    assert!(matches!(
        error,
        closed_sesame::SesameError::UnsupportedVersion { found: 2, .. }
    ));
    assert!(error.to_string().contains("recreate"));
}

#[test]
fn invalid_kdf_parameters_are_rejected() {
    let default = KdfConfig::default_password();
    for config in [
        KdfConfig {
            memory_kib: 0,
            ..default
        },
        KdfConfig {
            memory_kib: u32::MAX,
            ..default
        },
        KdfConfig {
            iterations: 0,
            ..default
        },
        KdfConfig {
            iterations: u32::MAX,
            ..default
        },
        KdfConfig {
            parallelism: 0,
            ..default
        },
        KdfConfig {
            parallelism: u32::MAX,
            ..default
        },
        KdfConfig {
            output_len: 31,
            ..default
        },
        KdfConfig {
            schema_version: 2,
            ..default
        },
    ] {
        assert!(config.validate().is_err(), "accepted {config:?}");
    }
    let mut bytes = ContainerHeader::new(
        ContainerFlags::NONE,
        [0; SALT_SIZE],
        [0; _HDR_NONCE],
        [0; TAG_SIZE],
        [0; WRAPPED_KEY_SIZE],
        default,
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    bytes[144..146].copy_from_slice(&u16::MAX.to_be_bytes());
    assert!(ContainerHeader::from_bytes(&bytes).is_err());
}

// ——— Container flags ———

#[test]
fn flags_password_protected() {
    let mut flags = ContainerFlags::NONE;
    assert!(!flags.is_password_protected());
    flags.set_password_protected(true);
    assert!(flags.is_password_protected());
    flags.set_password_protected(false);
    assert!(!flags.is_password_protected());
}

// ——— File entry ———

#[test]
fn file_entry_serialization_roundtrip() {
    let entry = FileEntry {
        path: "documents/taxes/2024.pdf".into(),
        original_path: Some("/home/user/Documents/taxes/2024.pdf".into()),
        size: 1024,
        start_page: 1,
        page_count: 1,
        crc32: 0xDEADBEEF,
        created_at: 1700000000,
        modified_at: 1700000000,
        has_symlink: true,
        is_directory: false,
    };
    let mut bytes = encode_file_table(std::slice::from_ref(&entry)).unwrap();
    bytes.resize(BLOCK_SIZE - _HDR_NONCE - TAG_SIZE, 0);
    let deserialized = decode_file_table(&bytes).unwrap().remove(0);
    assert_eq!(entry.path, deserialized.path);
    assert_eq!(entry.crc32, deserialized.crc32);
    assert_eq!(entry.has_symlink, deserialized.has_symlink);
}

#[test]
fn metadata_rejects_nonzero_tail_after_declared_payload() {
    let mut bytes = encode_file_table(&[]).unwrap();
    bytes.push(1);
    let error = decode_file_table(&bytes).unwrap_err();
    assert!(error.to_string().contains("non-zero bytes"));
}

#[test]
fn metadata_rejects_payload_length_beyond_block() {
    let mut bytes = encode_file_table(&[]).unwrap();
    bytes[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
    let error = decode_file_table(&bytes).unwrap_err();
    assert!(error.to_string().contains("length exceeds block"));
}

#[test]
fn allocation_bitmap_postcard_roundtrip_with_block_padding() {
    let mut bitmap = AllocationBitmap::new(1024);
    bitmap.allocate(17);
    bitmap.allocate(999);
    let mut bytes = encode_bitmap(&bitmap).unwrap();
    bytes.resize(BLOCK_SIZE - _HDR_NONCE - TAG_SIZE, 0);

    let decoded = decode_bitmap(&bytes).unwrap();
    assert_eq!(decoded.total_pages, 1024);
    assert!(decoded.is_allocated(17));
    assert!(decoded.is_allocated(999));
    assert_eq!(decoded.allocated_pages(), 2);
}

#[test]
fn allocation_bitmap_rejects_inconsistent_serialized_size() {
    let malformed = AllocationBitmap {
        bits: Vec::new(),
        total_pages: 8,
    };
    let bytes = encode_bitmap(&malformed).unwrap();
    let error = decode_bitmap(&bytes).unwrap_err();
    assert!(error.to_string().contains("expected 1"));
}
