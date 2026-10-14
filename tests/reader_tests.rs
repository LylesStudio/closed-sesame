//! Tests for container reading — open, verify, read files.

use closed_sesame::SesameError;
use closed_sesame::container::format::{BLOCK_SIZE, HEADER_SIZE, NONCE_SIZE};
use closed_sesame::container::io::{ContainerReader, ContainerWriter};
use closed_sesame::crypto::{KdfAlgorithm, KdfConfig, Salt};
use closed_sesame::keymgmt::KeyEncryptingKey;
use std::fs;
use tempfile::TempDir;

fn create_and_open(dir: &TempDir, name: &str) -> (ContainerReader, KeyEncryptingKey) {
    let path = dir.path().join(format!("{name}.sesame"));
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test-password", &salt, KdfConfig::default_password())
            .unwrap();
    let _writer = ContainerWriter::create(&path, &kek, salt, BLOCK_SIZE).unwrap();
    let reader = ContainerReader::open(&path, &kek).unwrap();
    (reader, kek)
}

#[test]
fn open_reads_headers_correctly() {
    let dir = TempDir::new().unwrap();
    let (reader, _) = create_and_open(&dir, "test");

    let header = reader.header();
    assert!(header.verify_magic());
    assert!(header.is_supported_version());
    assert!(header.flags.is_password_protected());
}

#[test]
fn open_rejects_wrong_password() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("wrongpw.sesame");
    let salt = Salt::generate();
    let config = KdfConfig::default_password();
    let kek_correct = KeyEncryptingKey::from_password("correct", &salt, config).unwrap();
    let kek_wrong = KeyEncryptingKey::from_password("wrong", &salt, config).unwrap();
    ContainerWriter::create(&path, &kek_correct, salt, BLOCK_SIZE).unwrap();

    let result = ContainerReader::open(&path, &kek_wrong);
    assert!(matches!(result, Err(SesameError::WrongPassword)));
}

#[test]
fn verify_on_fresh_container_passes() {
    let dir = TempDir::new().unwrap();
    let (reader, _) = create_and_open(&dir, "verify");

    let result = reader.verify();
    assert!(result.is_ok(), "verify failed: {:?}", result.err());
}

#[test]
fn verify_detects_file_table() {
    let dir = TempDir::new().unwrap();
    let (reader, _) = create_and_open(&dir, "detect");

    let entries = reader.list_files().unwrap();
    // Fresh container has empty file table
    assert!(entries.is_empty());
}

#[test]
fn verify_fails_on_corrupted_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("corrupt.sesame");
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test", &salt, KdfConfig::default_password()).unwrap();

    ContainerWriter::create(&path, &kek, salt, BLOCK_SIZE).unwrap();

    // Corrupt a byte inside Block 0 (File Table), after the nonce
    let mut bytes = fs::read(&path).unwrap();
    let corrupt_offset = HEADER_SIZE * 2 + NONCE_SIZE + 10; // inside encrypted part of Block 0
    bytes[corrupt_offset] ^= 0xFF;
    fs::write(&path, &bytes).unwrap();

    let reader = ContainerReader::open(&path, &kek).unwrap();
    // File table is corrupted, list_files should fail
    let result = reader.list_files();
    assert!(result.is_err());
}

#[test]
fn no_password_container_roundtrip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("no-password.sesame");
    let created = closed_sesame::app::vault::create(
        &path,
        1024 * 1024,
        closed_sesame::app::Credential::NoPassword,
    )
    .unwrap();
    let opened =
        closed_sesame::app::vault::open(&created, closed_sesame::app::Credential::NoPassword)
            .unwrap();
    assert_eq!(opened.reader.header().kdf_config, KdfConfig::no_password());
}

#[test]
fn header_config_drives_open_instead_of_current_default() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("custom-kdf.sesame");
    let explicit = KdfConfig {
        algorithm: KdfAlgorithm::Argon2id,
        schema_version: 1,
        memory_kib: 8 * 1024,
        iterations: 1,
        parallelism: 1,
        output_len: 32,
    };
    let created = closed_sesame::app::vault::create_with_kdf_config(
        &path,
        1024 * 1024,
        closed_sesame::app::Credential::Password("custom"),
        explicit,
    )
    .unwrap();
    assert_ne!(explicit, KdfConfig::default_password());

    let opened = closed_sesame::app::vault::open(
        &created,
        closed_sesame::app::Credential::Password("custom"),
    )
    .unwrap();
    assert_eq!(opened.reader.header().kdf_config, explicit);
}

#[test]
fn default_password_kdf_is_derivable() {
    let salt = Salt::from_slice(&[9; 32]).unwrap();
    let started = std::time::Instant::now();
    let result = closed_sesame::crypto::derive_key(
        "benchmark-compatible",
        &salt,
        &KdfConfig::default_password(),
    );
    println!("default Argon2id derivation: {:?}", started.elapsed());
    assert!(result.is_ok());
}
