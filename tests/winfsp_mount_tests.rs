//! End-to-end WinFsp mount tests on Windows.

#![cfg(all(target_os = "windows", feature = "mount"))]

use closed_sesame::container::ContainerWriter;
use closed_sesame::crypto::{KdfConfig, Salt};
use closed_sesame::keymgmt::KeyEncryptingKey;
use closed_sesame::mount::{MountCredentials, mount_vault};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn require_winfsp(test_name: &str) -> bool {
    match closed_sesame::mount::check_prerequisites() {
        Ok(()) => true,
        Err(error) => {
            eprintln!("SKIP {test_name}: WinFsp unavailable: {error}");
            false
        }
    }
}

fn pick_drive_letter() -> Option<String> {
    for letter in (b'T'..=b'Z').rev().map(|byte| byte as char) {
        let drive = format!("{letter}:");
        if !drive_type_is_allocated(&drive) {
            return Some(drive);
        }
    }
    None
}

fn drive_type_is_allocated(drive: &str) -> bool {
    let Ok(output) = Command::new("fsutil")
        .args(["fsinfo", "drivetype", drive])
        .output()
    else {
        return true;
    };
    let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    [
        "fixed",
        "remote",
        "removable",
        "network",
        "cd-rom",
        "ramdisk",
    ]
    .into_iter()
    .any(|kind| text.contains(kind))
}

fn verify_external_access(mount_point: &Path) {
    let drive = mount_point.to_string_lossy();
    let script = format!("Get-ChildItem -LiteralPath '{drive}\\' | Out-Null");
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .expect("spawn powershell");
    assert!(
        output.status.success(),
        "external access failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn winfsp_mount_read_file() {
    if !require_winfsp("winfsp_mount_read_file") {
        return;
    }
    let Some(mount_point) = pick_drive_letter() else {
        panic!("no free drive letter for WinFsp mount test");
    };

    let dir = TempDir::new().unwrap();
    let sbox_path = dir.path().join("winfsp_test.sesame");
    let password = "winfsp-test-pass";

    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password(password, &salt, KdfConfig::default_password()).unwrap();
    let mut writer = ContainerWriter::create(&sbox_path, &kek, salt, 5 * 1024 * 1024).unwrap();

    let test_content = b"Hello from inside the WinFsp vault!\n";
    let src = dir.path().join("secret.txt");
    fs::write(&src, test_content).unwrap();
    writer.add_file(&src, "secret.txt").unwrap();
    drop(writer);

    let mut mounted = mount_vault(
        MountCredentials::Password(password.to_owned()),
        &sbox_path,
        &mount_point,
    )
    .unwrap_or_else(|error| panic!("mount to {mount_point} failed: {error}"));

    std::thread::sleep(std::time::Duration::from_millis(200));

    let mounted_file = PathBuf::from(format!("{}\\secret.txt", mount_point));
    assert!(mounted_file.exists(), "mounted file should exist");

    let entries: Vec<_> = fs::read_dir(PathBuf::from(format!("{}\\", mount_point)))
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        entries.contains(&"secret.txt".to_string()),
        "directory listing: {entries:?}"
    );

    verify_external_access(Path::new(&mount_point));

    let mut buf = Vec::new();
    fs::File::open(&mounted_file)
        .unwrap()
        .read_to_end(&mut buf)
        .unwrap();
    assert_eq!(buf, test_content);

    mounted.unmount().unwrap();
}

/// Write path: create a new file through the WinFsp mount and verify it
/// lands in the container after unmount.
#[test]
fn winfsp_mount_write_new_file() {
    if !require_winfsp("winfsp_mount_write_new_file") {
        return;
    }
    let Some(mount_point) = pick_drive_letter() else {
        panic!("no free drive letter for WinFsp mount test");
    };

    let dir = TempDir::new().unwrap();
    let sbox_path = dir.path().join("winfsp_write.sesame");
    let password = "winfsp-write-pass";
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password(password, &salt, KdfConfig::default_password()).unwrap();
    ContainerWriter::create(&sbox_path, &kek, salt, 5 * 1024 * 1024).unwrap();

    let mut mounted = mount_vault(
        MountCredentials::Password(password.to_owned()),
        &sbox_path,
        &mount_point,
    )
    .unwrap_or_else(|error| panic!("mount to {mount_point} failed: {error}"));
    std::thread::sleep(std::time::Duration::from_millis(200));

    let content = b"created through the writable WinFsp mount\n";
    let mounted_file = PathBuf::from(format!("{}\\created.txt", mount_point));
    fs::write(&mounted_file, content).unwrap();
    assert_eq!(fs::read(&mounted_file).unwrap(), content);

    mounted.unmount().unwrap();

    let reader = closed_sesame::ContainerReader::open(&sbox_path, &kek).unwrap();
    let entries = reader.list_files().unwrap();
    let entry = entries
        .iter()
        .find(|entry| entry.path == "created.txt")
        .expect("created.txt must exist in the container");
    assert_eq!(reader.read_file(entry).unwrap(), content);
}

/// Write path: overwrite an existing file through the mount; the old content
/// must be fully replaced (truncate semantics), never corrupted or duplicated.
#[test]
fn winfsp_mount_edit_existing_file() {
    if !require_winfsp("winfsp_mount_edit_existing_file") {
        return;
    }
    let Some(mount_point) = pick_drive_letter() else {
        panic!("no free drive letter for WinFsp mount test");
    };

    let dir = TempDir::new().unwrap();
    let sbox_path = dir.path().join("winfsp_edit.sesame");
    let password = "winfsp-edit-pass";
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password(password, &salt, KdfConfig::default_password()).unwrap();
    let mut writer = ContainerWriter::create(&sbox_path, &kek, salt, 5 * 1024 * 1024).unwrap();
    let src = dir.path().join("doc.txt");
    fs::write(&src, b"original content that is quite long\n").unwrap();
    writer.add_file(&src, "doc.txt").unwrap();
    drop(writer);

    let mut mounted = mount_vault(
        MountCredentials::Password(password.to_owned()),
        &sbox_path,
        &mount_point,
    )
    .unwrap_or_else(|error| panic!("mount to {mount_point} failed: {error}"));
    std::thread::sleep(std::time::Duration::from_millis(200));

    let replacement = b"short\n";
    let mounted_file = PathBuf::from(format!("{}\\doc.txt", mount_point));
    fs::write(&mounted_file, replacement).unwrap();

    mounted.unmount().unwrap();

    let reader = closed_sesame::ContainerReader::open(&sbox_path, &kek).unwrap();
    let entries = reader.list_files().unwrap();
    let matching: Vec<_> = entries
        .iter()
        .filter(|entry| entry.path == "doc.txt")
        .collect();
    assert_eq!(matching.len(), 1, "exactly one doc.txt entry");
    assert_eq!(reader.read_file(matching[0]).unwrap(), replacement);
}

/// Write path: mkdir, nested write, rename, delete file and directory through
/// the mount, verified against the container afterwards.
#[test]
fn winfsp_mount_mkdir_rename_delete() {
    if !require_winfsp("winfsp_mount_mkdir_rename_delete") {
        return;
    }
    let Some(mount_point) = pick_drive_letter() else {
        panic!("no free drive letter for WinFsp mount test");
    };

    let dir = TempDir::new().unwrap();
    let sbox_path = dir.path().join("winfsp_tree.sesame");
    let password = "winfsp-tree-pass";
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password(password, &salt, KdfConfig::default_password()).unwrap();
    ContainerWriter::create(&sbox_path, &kek, salt, 5 * 1024 * 1024).unwrap();

    let mut mounted = mount_vault(
        MountCredentials::Password(password.to_owned()),
        &sbox_path,
        &mount_point,
    )
    .unwrap_or_else(|error| panic!("mount to {mount_point} failed: {error}"));
    std::thread::sleep(std::time::Duration::from_millis(200));

    let root = PathBuf::from(format!("{}\\", mount_point));
    fs::create_dir(root.join("docs")).unwrap();
    fs::write(root.join("docs\\a.txt"), b"nested\n").unwrap();
    fs::rename(root.join("docs\\a.txt"), root.join("docs\\b.txt")).unwrap();
    assert!(root.join("docs\\b.txt").exists());
    fs::remove_file(root.join("docs\\b.txt")).unwrap();
    fs::remove_dir(root.join("docs")).unwrap();

    mounted.unmount().unwrap();

    let reader = closed_sesame::ContainerReader::open(&sbox_path, &kek).unwrap();
    let entries = reader.list_files().unwrap();
    assert!(
        entries.is_empty(),
        "container should be empty, found: {:?}",
        entries.iter().map(|entry| &entry.path).collect::<Vec<_>>()
    );
}
