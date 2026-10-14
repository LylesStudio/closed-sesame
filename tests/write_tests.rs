//! Tests for file write operations — V2 page-level allocation.

use closed_sesame::container::format::FileEntry;
use closed_sesame::container::io::{ContainerReader, ContainerWriter};
use closed_sesame::crypto::{KdfConfig, Salt};
use closed_sesame::keymgmt::KeyEncryptingKey;
use std::fs;
use tempfile::TempDir;

#[test]
fn add_single_small_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("write.sesame");
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test", &salt, KdfConfig::default_password()).unwrap();
    let mut writer = ContainerWriter::create(&path, &kek, salt, 50 * 1024 * 1024).unwrap();

    let src = dir.path().join("hello.txt");
    fs::write(&src, b"hello world").unwrap();

    let entry = writer.add_file(&src, "hello.txt").unwrap();
    assert_eq!(entry.path, "hello.txt");
    assert_eq!(entry.size, 11);
    assert!(!entry.is_directory);

    // Verify page-count math: 11 bytes needs 1 page (PAGE_PLAINTEXT > 11)
    assert_eq!(entry.page_count, FileEntry::pages_for_size(11));

    drop(writer);

    // Re-open and verify
    let reader = ContainerReader::open(&path, &kek).unwrap();
    let files = reader.list_files().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "hello.txt");
    assert_eq!(files[0].size, 11);
}

#[test]
fn add_file_and_verify_contents() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("content.sesame");
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test", &salt, KdfConfig::default_password()).unwrap();
    let mut writer = ContainerWriter::create(&path, &kek, salt, 50 * 1024 * 1024).unwrap();

    let original = b"test content for verification";
    let src = dir.path().join("data.bin");
    fs::write(&src, original).unwrap();

    writer.add_file(&src, "data.bin").unwrap();
    drop(writer);

    let reader = ContainerReader::open(&path, &kek).unwrap();
    let files = reader.list_files().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].size, original.len() as u64);

    // Read file data back
    let data = reader.read_file(&files[0]).unwrap();
    assert_eq!(data, original);
}

#[test]
fn add_empty_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("empty.sesame");
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test", &salt, KdfConfig::default_password()).unwrap();
    let mut writer = ContainerWriter::create(&path, &kek, salt, 1024 * 1024).unwrap();

    let src = dir.path().join("empty.txt");
    fs::write(&src, b"").unwrap();

    let entry = writer.add_file(&src, "empty.txt").unwrap();
    assert_eq!(entry.size, 0);
    // Empty files still get 1 page
    assert_eq!(entry.page_count, 1);

    drop(writer);

    let reader = ContainerReader::open(&path, &kek).unwrap();
    let files = reader.list_files().unwrap();
    assert_eq!(files[0].size, 0);

    // Read back empty file
    let data = reader.read_file(&files[0]).unwrap();
    assert!(data.is_empty());
}

#[test]
fn add_file_disk_full() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("tiny.sesame");
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test", &salt, KdfConfig::default_password()).unwrap();
    // Create a tiny container with minimal data blocks
    let mut writer = ContainerWriter::create(&path, &kek, salt, 1024).unwrap();

    let src = dir.path().join("big.bin");
    fs::write(&src, vec![0u8; 10 * 1024 * 1024]).unwrap(); // 10MB, won't fit

    let result = writer.add_file(&src, "big.bin");
    assert!(result.is_err());
}

#[test]
fn add_multiple_files() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("multi.sesame");
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test", &salt, KdfConfig::default_password()).unwrap();
    let mut writer = ContainerWriter::create(&path, &kek, salt, 50 * 1024 * 1024).unwrap();

    let mut entries: Vec<FileEntry> = Vec::new();
    let contents: Vec<Vec<u8>> = (0..5)
        .map(|i| format!("content {i}").into_bytes())
        .collect();

    for (i, content) in contents.iter().enumerate() {
        let src = dir.path().join(format!("file{i}.txt"));
        fs::write(&src, content).unwrap();
        let entry = writer.add_file(&src, &format!("file{i}.txt")).unwrap();
        entries.push(entry);
    }
    drop(writer);

    let reader = ContainerReader::open(&path, &kek).unwrap();
    let files = reader.list_files().unwrap();
    assert_eq!(files.len(), 5);

    // Verify each file's content
    for file in &files {
        let data = reader.read_file(file).unwrap();
        // Find the matching expected content
        let parsed_i: usize = file
            .path
            .trim_start_matches("file")
            .trim_end_matches(".txt")
            .parse()
            .unwrap();
        assert_eq!(data, contents[parsed_i]);
    }
}

#[test]
fn add_file_larger_than_one_page() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("large.sesame");
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test", &salt, KdfConfig::default_password()).unwrap();

    // Create data that spans multiple pages
    let page_size = closed_sesame::container::format::PAGE_PLAINTEXT;
    let data = vec![0xABu8; page_size * 3 + 500]; // ~12KB
    let mut writer = ContainerWriter::create(&path, &kek, salt, data.len() * 2).unwrap();

    let src = dir.path().join("large.bin");
    fs::write(&src, &data).unwrap();

    let entry = writer.add_file(&src, "large.bin").unwrap();
    assert_eq!(entry.size, data.len() as u64);
    assert_eq!(
        entry.page_count,
        FileEntry::pages_for_size(data.len() as u64)
    );

    drop(writer);

    // Verify content
    let reader = ContainerReader::open(&path, &kek).unwrap();
    let files = reader.list_files().unwrap();
    let read_back = reader.read_file(&files[0]).unwrap();
    assert_eq!(read_back, data);
}

#[test]
fn power_fail_simulation() {
    // Verify that a corrupted .sesame.NEW doesn't affect the original
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("power.sesame");
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test", &salt, KdfConfig::default_password()).unwrap();
    let mut writer = ContainerWriter::create(&path, &kek, salt, 5 * 1024 * 1024).unwrap();

    let src = dir.path().join("good.txt");
    fs::write(&src, b"this should survive").unwrap();
    writer.add_file(&src, "good.txt").unwrap();
    drop(writer);

    // Simulate power fail: create corrupted .NEW
    let new_path = path.with_extension("sesame.NEW");
    fs::copy(&path, &new_path).unwrap();
    let mut bytes = fs::read(&new_path).unwrap();
    if bytes.len() > 1000 {
        bytes[500] ^= 0xFF;
    }
    fs::write(&new_path, &bytes).unwrap();
    // Don't rename — .NEW stays orphaned

    // Original must still be valid
    let reader = ContainerReader::open(&path, &kek).unwrap();
    let files = reader.list_files().unwrap();
    assert_eq!(
        files.len(),
        1,
        "original container should be intact after power fail"
    );
    assert_eq!(files[0].path, "good.txt");

    // Verify data is readable
    let data = reader.read_file(&files[0]).unwrap();
    assert_eq!(data, b"this should survive");
}
