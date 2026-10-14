//! WinFsp end-to-end tests for directory symlinks.

#![cfg(all(target_os = "windows", feature = "mount"))]

use closed_sesame::app::symlink::SymlinkRegistry;
use closed_sesame::container::ContainerWriter;
use closed_sesame::crypto::{KdfConfig, Salt};
use closed_sesame::keymgmt::KeyEncryptingKey;
use closed_sesame::mount::{MountCredentials, mount_vault};
use std::fs;
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

fn symlink_dir_permissions_available() -> bool {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("target");
    let link = temp.path().join("link");
    fs::create_dir_all(&target).unwrap();
    if std::os::windows::fs::symlink_dir(&target, &link).is_err() {
        return false;
    }
    let _ = fs::remove_dir(link);
    true
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

#[test]
fn winfsp_add_dir_dir_symlink_e2e() {
    if !require_winfsp("winfsp_add_dir_dir_symlink_e2e") {
        return;
    }
    if !symlink_dir_permissions_available() {
        eprintln!("SKIP winfsp_add_dir_dir_symlink_e2e: directory symlink permission denied");
        return;
    }
    let Some(mount_point) = pick_drive_letter() else {
        panic!("no free drive letter for WinFsp mount test");
    };

    let dir = TempDir::new().unwrap();
    let sbox_path = dir.path().join("winfsp_symlink.sesame");
    let password = "winfsp-symlink-pass";

    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password(password, &salt, KdfConfig::default_password()).unwrap();
    let mut writer = ContainerWriter::create(&sbox_path, &kek, salt, 5 * 1024 * 1024).unwrap();
    writer.mkdir("project").unwrap();
    writer
        .add_file_data(
            b"mounted nested content",
            "project/nested/file.txt",
            None,
            false,
        )
        .unwrap();
    drop(writer);

    let mut mounted = mount_vault(
        MountCredentials::Password(password.to_owned()),
        &sbox_path,
        &mount_point,
    )
    .unwrap_or_else(|error| panic!("mount to {mount_point} failed: {error}"));

    std::thread::sleep(std::time::Duration::from_millis(200));

    let source_dir = dir.path().join("project");
    fs::create_dir_all(source_dir.join("nested")).unwrap();
    fs::write(source_dir.join("nested/file.txt"), b"local content").unwrap();

    let mut registry = SymlinkRegistry::default();
    registry
        .move_dir_with_symlink(&source_dir, Path::new(&mount_point), "project")
        .unwrap_or_else(|error| panic!("move_dir_with_symlink failed: {error}"));

    assert!(
        source_dir
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink(),
        "source directory should be replaced by a symlink"
    );
    assert_eq!(
        fs::read_to_string(source_dir.join("nested/file.txt")).unwrap(),
        "mounted nested content"
    );

    let mounted_file = PathBuf::from(format!("{}\\project\\nested\\file.txt", mount_point));
    assert_eq!(
        fs::read_to_string(&mounted_file).unwrap(),
        "mounted nested content"
    );

    mounted.unmount().unwrap();
}
