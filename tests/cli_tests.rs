//! Integration tests for the `sesame` CLI.
//!
//! Tests the binary end-to-end by invoking its commands.
#![cfg(all(feature = "cli", feature = "mount"))]

mod common;

use common::{assert_success, create_no_password, rewrite_all_header_versions, run};
use std::fs;
use tempfile::TempDir;

fn sesame_binary() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_sesame"))
}

#[test]
fn create_command_requires_name() {
    let output = run(&sesame_binary(), &["create"]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("NAME") || stderr.contains("required") || stderr.contains("error"));
}

#[test]
fn no_password_create_add_list_extract_remove_verify_e2e() {
    let dir = TempDir::new().unwrap();
    let binary = sesame_binary();
    let container = create_no_password(&binary, dir.path(), "cli-e2e");
    let source = dir.path().join("source.txt");
    let extracted = dir.path().join("extracted.txt");
    let contents = b"closed-sesame CLI end-to-end";
    fs::write(&source, contents).unwrap();

    let add = run(
        &binary,
        &[
            "add",
            container.to_str().unwrap(),
            source.to_str().unwrap(),
            "--name",
            "inside.txt",
            "--no-password",
        ],
    );
    assert_success(&add);

    let list = run(
        &binary,
        &["list", container.to_str().unwrap(), "--no-password"],
    );
    assert_success(&list);
    assert!(String::from_utf8_lossy(&list.stdout).contains("inside.txt"));

    let extract = run(
        &binary,
        &[
            "extract",
            container.to_str().unwrap(),
            "inside.txt",
            "--output",
            extracted.to_str().unwrap(),
            "--no-password",
        ],
    );
    assert_success(&extract);
    assert_eq!(fs::read(&extracted).unwrap(), contents);

    // Default extract keeps the file inside the container.
    let list_after_copy = run(
        &binary,
        &["list", container.to_str().unwrap(), "--no-password"],
    );
    assert_success(&list_after_copy);
    assert!(
        String::from_utf8_lossy(&list_after_copy.stdout).contains("inside.txt"),
        "extract without --move must leave the entry in the container"
    );

    let moved = dir.path().join("moved.txt");
    let extract_move = run(
        &binary,
        &[
            "extract",
            container.to_str().unwrap(),
            "inside.txt",
            "--output",
            moved.to_str().unwrap(),
            "--move",
            "--no-password",
        ],
    );
    assert_success(&extract_move);
    assert_eq!(fs::read(&moved).unwrap(), contents);
    let list_after_move = run(
        &binary,
        &["list", container.to_str().unwrap(), "--no-password"],
    );
    assert_success(&list_after_move);
    assert!(
        !String::from_utf8_lossy(&list_after_move.stdout).contains("inside.txt"),
        "extract --move must remove the entry from the container"
    );

    let readd = run(
        &binary,
        &[
            "add",
            container.to_str().unwrap(),
            source.to_str().unwrap(),
            "--name",
            "inside.txt",
            "--no-password",
        ],
    );
    assert_success(&readd);
    let remove = run(
        &binary,
        &[
            "remove",
            container.to_str().unwrap(),
            "inside.txt",
            "--no-password",
        ],
    );
    assert_success(&remove);

    let verify = run(
        &binary,
        &["verify", container.to_str().unwrap(), "--no-password"],
    );
    assert_success(&verify);

    let list = run(
        &binary,
        &["list", container.to_str().unwrap(), "--no-password"],
    );
    assert_success(&list);
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(!stdout.contains("inside.txt"));
    assert!(stdout.contains("container") || stdout.trim().is_empty() || stdout.contains('('));
}

#[test]
fn v2_container_is_rejected_with_failure_exit_code() {
    let dir = TempDir::new().unwrap();
    let binary = sesame_binary();
    let container = create_no_password(&binary, dir.path(), "v2");
    rewrite_all_header_versions(&container, 2);

    let output = run(
        &binary,
        &["verify", container.to_str().unwrap(), "--no-password"],
    );

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("2") || stderr.contains("version"));
}

#[test]
fn invalid_size_returns_failure_exit_code() {
    let dir = TempDir::new().unwrap();
    let output = run(
        &sesame_binary(),
        &[
            "create",
            dir.path().join("bad").to_str().unwrap(),
            "--size",
            "not-a-size",
            "--no-password",
        ],
    );

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid"));
}

#[test]
fn missing_container_returns_failure_exit_code() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing.sesame");
    let output = run(
        &sesame_binary(),
        &["list", missing.to_str().unwrap(), "--no-password"],
    );

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.to_ascii_lowercase().contains("error")
            || stderr.contains("错误")
            || stderr.contains("エラー")
    );
}

#[test]
fn recursive_add_uses_source_dir_name() {
    let dir = TempDir::new().unwrap();
    let binary = sesame_binary();
    let container = create_no_password(&binary, dir.path(), "recursive-add");
    let source_dir = dir.path().join("sourcedir");
    fs::create_dir_all(source_dir.join("nested")).unwrap();
    fs::write(source_dir.join("nested.txt"), b"nested").unwrap();

    let add = run(
        &binary,
        &[
            "add",
            container.to_str().unwrap(),
            source_dir.to_str().unwrap(),
            "--recursive",
            "--no-password",
        ],
    );
    assert_success(&add);

    let list = run(
        &binary,
        &["list", container.to_str().unwrap(), "--no-password"],
    );
    assert_success(&list);
    let stdout = String::from_utf8_lossy(&list.stdout);
    assert!(stdout.contains("sourcedir/nested.txt"));
    assert!(!stdout.contains(" nested.txt") || stdout.contains("sourcedir/nested.txt"));
}

#[test]
fn extract_directory_via_cli() {
    let dir = TempDir::new().unwrap();
    let binary = sesame_binary();
    let container = create_no_password(&binary, dir.path(), "dir-extract");
    let project = dir.path().join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("readme.txt"), b"readme").unwrap();

    let add = run(
        &binary,
        &[
            "add",
            container.to_str().unwrap(),
            project.to_str().unwrap(),
            "--recursive",
            "--no-password",
        ],
    );
    assert_success(&add);

    let output = dir.path().join("out");
    fs::create_dir_all(&output).unwrap();
    let extract = run(
        &binary,
        &[
            "extract",
            container.to_str().unwrap(),
            "project",
            "--output",
            output.to_str().unwrap(),
            "--no-password",
        ],
    );
    assert_success(&extract);
    assert_eq!(
        fs::read(output.join("project").join("readme.txt")).unwrap(),
        b"readme"
    );
}

#[test]
fn extract_conflicts_on_existing_file() {
    let dir = TempDir::new().unwrap();
    let binary = sesame_binary();
    let container = create_no_password(&binary, dir.path(), "conflict");
    let source = dir.path().join("doc.txt");
    fs::write(&source, b"vault").unwrap();

    let add = run(
        &binary,
        &[
            "add",
            container.to_str().unwrap(),
            source.to_str().unwrap(),
            "--name",
            "doc.txt",
            "--no-password",
        ],
    );
    assert_success(&add);

    let output = dir.path().join("out");
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("doc.txt"), b"existing").unwrap();

    let extract = run(
        &binary,
        &[
            "extract",
            container.to_str().unwrap(),
            "doc.txt",
            "--output",
            output.to_str().unwrap(),
            "--no-password",
        ],
    );
    assert_ne!(extract.status.code(), Some(0));
    assert_eq!(fs::read(output.join("doc.txt")).unwrap(), b"existing");
}
