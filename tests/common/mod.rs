#![allow(dead_code)]

use closed_sesame::container::format::{
    BLOCK_SIZE, ContainerHeader, FileEntry, HEADER_SIZE, NONCE_SIZE, PAGE_SIZE,
};
use closed_sesame::container::metadata::{decode_file_table, encode_file_table};
use closed_sesame::crypto::{self, Salt};
use closed_sesame::keymgmt::KeyEncryptingKey;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub fn run(binary: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(binary);
    command.args(["--lang", "en"]).args(args);
    command.env("CLOSED_SESAME_LANG", "en");
    command.output().expect("failed to run test binary")
}

pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed with {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn create_no_password(binary: &Path, directory: &Path, stem: &str) -> PathBuf {
    let requested = directory.join(stem);
    let output = run(
        binary,
        &[
            "create",
            requested.to_str().unwrap(),
            "--size",
            "64KB",
            "--no-password",
        ],
    );
    assert_success(&output);
    requested.with_extension("sesame")
}

pub fn rewrite_all_header_versions(path: &Path, version: u16) {
    let mut bytes = fs::read(path).unwrap();
    let footer = bytes.len() - HEADER_SIZE;
    for offset in [0, HEADER_SIZE, footer] {
        bytes[offset + 16..offset + 18].copy_from_slice(&version.to_be_bytes());
    }
    fs::write(path, bytes).unwrap();
}

pub fn corrupt_data_page(path: &Path, page: u64) {
    let offset = HEADER_SIZE as u64 * 2
        + BLOCK_SIZE as u64 * 2
        + page * PAGE_SIZE as u64
        + NONCE_SIZE as u64;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    file.seek(SeekFrom::Start(offset)).unwrap();
    let mut byte = [0];
    file.read_exact(&mut byte).unwrap();
    byte[0] ^= 0x80;
    file.seek(SeekFrom::Start(offset)).unwrap();
    file.write_all(&byte).unwrap();
}

pub fn corrupt_entry_crc(path: &Path, target: &str) {
    let header = ContainerHeader::from_bytes(&fs::read(path).unwrap()[..HEADER_SIZE]).unwrap();
    let salt = Salt::from_slice(&header.salt).unwrap();
    let kek = KeyEncryptingKey::no_password(&salt, header.kdf_config).unwrap();
    let master_key = kek
        .unwrap_master_key(&header.wrapped_master_key, &header.nonce)
        .unwrap();

    let metadata_offset = HEADER_SIZE as u64 * 2;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    file.seek(SeekFrom::Start(metadata_offset)).unwrap();
    let mut block = vec![0; BLOCK_SIZE];
    file.read_exact(&mut block).unwrap();
    let nonce: [u8; NONCE_SIZE] = block[..NONCE_SIZE].try_into().unwrap();
    let plaintext = crypto::decrypt(&master_key, &block[NONCE_SIZE..], &nonce).unwrap();
    let mut entries: Vec<FileEntry> = decode_file_table(&plaintext).unwrap();
    let entry = entries
        .iter_mut()
        .find(|entry| entry.path == target)
        .unwrap();
    entry.crc32 ^= u32::MAX;

    let serialized = encode_file_table(&entries).unwrap();
    let mut padded = vec![0; BLOCK_SIZE - NONCE_SIZE - crypto::TAG_SIZE];
    padded[..serialized.len()].copy_from_slice(&serialized);
    let (ciphertext, nonce) = crypto::encrypt(&master_key, &padded).unwrap();
    let mut rewritten = vec![0; BLOCK_SIZE];
    rewritten[..NONCE_SIZE].copy_from_slice(&nonce);
    rewritten[NONCE_SIZE..NONCE_SIZE + ciphertext.len()].copy_from_slice(&ciphertext);
    file.seek(SeekFrom::Start(metadata_offset)).unwrap();
    file.write_all(&rewritten).unwrap();
}
