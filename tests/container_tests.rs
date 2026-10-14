//! Integration tests for container I/O — create, open, read, write, verify.

use closed_sesame::container::format::{BLOCK_SIZE, ContainerHeader, HEADER_SIZE};
use closed_sesame::container::io::ContainerWriter;
use closed_sesame::crypto::{KdfConfig, Salt};
use closed_sesame::keymgmt::KeyEncryptingKey;
use std::fs;
use tempfile::TempDir;

fn create_test_container(dir: &TempDir, name: &str) -> (std::path::PathBuf, KeyEncryptingKey) {
    let path = dir.path().join(format!("{name}.sesame"));
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test-password", &salt, KdfConfig::default_password())
            .unwrap();
    let _writer = ContainerWriter::create(&path, &kek, salt, 1024 * 1024).unwrap();
    (path, kek)
}

#[test]
fn create_container_produces_valid_file() {
    let dir = TempDir::new().unwrap();
    let (path, _kek) = create_test_container(&dir, "test");
    assert!(path.exists());

    let metadata = fs::metadata(&path).unwrap();
    assert!(metadata.len() >= (HEADER_SIZE * 3 + BLOCK_SIZE) as u64);

    let bytes = fs::read(&path).unwrap();
    let header_a = ContainerHeader::from_bytes(&bytes[..HEADER_SIZE]).unwrap();
    assert!(header_a.verify_magic());
}

#[test]
fn create_container_header_redundancy() {
    let dir = TempDir::new().unwrap();
    let (path, _kek) = create_test_container(&dir, "test");
    let bytes = fs::read(&path).unwrap();

    let header_a = ContainerHeader::from_bytes(&bytes[0..HEADER_SIZE]).unwrap();
    let header_b = ContainerHeader::from_bytes(&bytes[HEADER_SIZE..HEADER_SIZE * 2]).unwrap();
    let footer_offset = bytes.len() - HEADER_SIZE;
    let footer = ContainerHeader::from_bytes(&bytes[footer_offset..]).unwrap();

    assert_eq!(header_a.container_id, header_b.container_id);
    assert_eq!(header_a.container_id, footer.container_id);
    assert_eq!(header_a.magic, header_b.magic);
    assert_eq!(header_a.magic, footer.magic);
    assert_eq!(header_a.salt, header_b.salt);
}

#[test]
fn create_container_has_correct_size() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("sized.sesame");
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test-password", &salt, KdfConfig::default_password())
            .unwrap();

    let data_size = BLOCK_SIZE * 2;
    let _writer = ContainerWriter::create(&path, &kek, salt, data_size).unwrap();

    let expected_min = (HEADER_SIZE * 3 + BLOCK_SIZE + data_size) as u64;
    let actual = fs::metadata(&path).unwrap().len();
    assert!(
        actual >= expected_min,
        "expected >= {expected_min}, got {actual}"
    );
}

#[test]
fn create_container_fails_if_file_exists() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("exists.sesame");
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test-password", &salt, KdfConfig::default_password())
            .unwrap();

    ContainerWriter::create(&path, &kek, Salt::generate(), 1024).unwrap();
    let result = ContainerWriter::create(&path, &kek, Salt::generate(), 1024);
    assert!(result.is_err());
}

#[test]
fn container_header_password_protected_flag() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("flagged.sesame");
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test-password", &salt, KdfConfig::default_password())
            .unwrap();

    let _writer = ContainerWriter::create(&path, &kek, salt, BLOCK_SIZE).unwrap();

    let bytes = fs::read(&path).unwrap();
    let header = ContainerHeader::from_bytes(&bytes[..HEADER_SIZE]).unwrap();
    assert!(
        header.flags.is_password_protected(),
        "password_protected flag should be set"
    );
}
