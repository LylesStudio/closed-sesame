#![cfg(feature = "cli")]

use closed_sesame::container::ContainerWriter;
use closed_sesame::container::path::{join, normalize};
use closed_sesame::crypto::{KdfConfig, Salt};
use closed_sesame::keymgmt::KeyEncryptingKey;

#[test]
fn normalize_rejects_parent_segments() {
    assert!(normalize("../secret").is_err());
    assert_eq!(join("a", "b").unwrap(), "a/b");
}

#[test]
fn writer_mkdir_remove_rename() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tree.sesame");
    let kek = KeyEncryptingKey::no_password(&Salt::generate(), KdfConfig::no_password()).unwrap();
    ContainerWriter::create(&path, &kek, Salt::generate(), 1024 * 1024).unwrap();
    let mut writer = ContainerWriter::open(&path, &kek).unwrap();
    writer.mkdir("projects/docs").unwrap();
    writer
        .add_file_data(b"hello", "projects/docs/readme.txt", None, false)
        .unwrap();
    writer
        .rename_path("projects/docs/readme.txt", "projects/docs/guide.txt")
        .unwrap();
    let entries = writer.entries().to_vec();
    assert!(
        entries
            .iter()
            .any(|entry| entry.path == "projects/docs/guide.txt")
    );
    writer.remove_path("projects").unwrap();
    assert!(
        ContainerWriter::open(&path, &kek)
            .unwrap()
            .entries()
            .is_empty()
    );
}
