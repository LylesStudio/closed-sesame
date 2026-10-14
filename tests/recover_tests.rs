//! End-to-end tests for the standalone recovery binary.
#![cfg(feature = "recover")]

mod common;

use closed_sesame::app::{credentials::Credential, vault};
use common::{
    assert_success, corrupt_data_page, corrupt_entry_crc, rewrite_all_header_versions, run,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn recover_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sesame-recover"))
}

fn create_vault(directory: &Path, files: &[(&str, &[u8])]) -> PathBuf {
    let container = vault::create(
        directory.join("recovery.sesame"),
        64 * 1024,
        Credential::NoPassword,
    )
    .unwrap();
    for (name, contents) in files {
        let source = directory.join(format!("source-{name}"));
        fs::write(&source, contents).unwrap();
        vault::add(&container, Credential::NoPassword, &source, Some(name)).unwrap();
    }
    container
}

#[test]
fn no_password_verify_list_and_extract_e2e() {
    let dir = TempDir::new().unwrap();
    let container = create_vault(dir.path(), &[("hello.txt", b"recover me")]);
    let output_dir = dir.path().join("output");
    let binary = recover_binary();

    let verify = run(
        &binary,
        &["verify", container.to_str().unwrap(), "--no-password"],
    );
    assert_success(&verify);
    assert!(String::from_utf8_lossy(&verify.stdout).contains("verified successfully"));

    let list = run(
        &binary,
        &["list", container.to_str().unwrap(), "--no-password"],
    );
    assert_success(&list);
    assert!(String::from_utf8_lossy(&list.stdout).contains("hello.txt"));

    let extract = run(
        &binary,
        &[
            "extract",
            container.to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
            "--no-password",
        ],
    );
    assert_success(&extract);
    assert_eq!(
        fs::read(output_dir.join("hello.txt")).unwrap(),
        b"recover me"
    );
}

#[test]
fn v2_container_is_rejected_without_prompting() {
    let dir = TempDir::new().unwrap();
    let container = create_vault(dir.path(), &[]);
    rewrite_all_header_versions(&container, 2);

    let output = run(
        &recover_binary(),
        &["verify", container.to_str().unwrap(), "--no-password"],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported container version 2"));
}

#[test]
fn extract_continues_after_a_damaged_file() {
    let dir = TempDir::new().unwrap();
    let container = create_vault(
        dir.path(),
        &[("good.txt", b"still readable"), ("damaged.txt", b"broken")],
    );
    let damaged = vault::list(&container, Credential::NoPassword)
        .unwrap()
        .into_iter()
        .find(|entry| entry.path == "damaged.txt")
        .unwrap();
    corrupt_data_page(&container, damaged.start_page);
    let output_dir = dir.path().join("best-effort");

    let output = run(
        &recover_binary(),
        &[
            "extract",
            container.to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
            "--no-password",
        ],
    );

    assert_success(&output);
    assert_eq!(
        fs::read(output_dir.join("good.txt")).unwrap(),
        b"still readable"
    );
    assert!(!output_dir.join("damaged.txt").exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 extracted, 1 failed, 0 skipped"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("damaged.txt"));
}

#[test]
fn extract_can_skip_a_crc_mismatch() {
    let dir = TempDir::new().unwrap();
    let container = create_vault(dir.path(), &[("checksum.txt", b"valid encrypted data")]);
    corrupt_entry_crc(&container, "checksum.txt");
    let output_dir = dir.path().join("skip-corrupt");

    let output = run(
        &recover_binary(),
        &[
            "extract",
            container.to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
            "--skip-corrupted",
            "--no-password",
        ],
    );

    assert_success(&output);
    assert!(!output_dir.join("checksum.txt").exists());
    assert!(String::from_utf8_lossy(&output.stdout).contains("0 extracted, 0 failed, 1 skipped"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("CRC32 mismatch"));
}

#[test]
fn extract_all_reports_conflict_as_failed() {
    let dir = TempDir::new().unwrap();
    let container = create_vault(dir.path(), &[("doc.txt", b"vault")]);
    let output_dir = dir.path().join("output");
    fs::create_dir_all(&output_dir).unwrap();
    fs::write(output_dir.join("doc.txt"), b"existing").unwrap();

    let output = run(
        &recover_binary(),
        &[
            "extract",
            container.to_str().unwrap(),
            "--output",
            output_dir.to_str().unwrap(),
            "--no-password",
        ],
    );

    assert_success(&output);
    assert_eq!(fs::read(output_dir.join("doc.txt")).unwrap(), b"existing");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0 extracted, 1 failed, 0 skipped"));
}
