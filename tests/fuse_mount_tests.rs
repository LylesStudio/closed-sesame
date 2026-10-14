//! End-to-end FUSE mount tests (Linux).
//!
//! These tests create real containers, mount them via FUSE, and verify
//! files are readable and writable through the mount point.

#[cfg(target_os = "linux")]
mod fuse_mount_tests {
    use closed_sesame::app::credentials::Credential;
    use closed_sesame::container::ContainerWriter;
    use closed_sesame::crypto::{KdfConfig, Salt};
    use closed_sesame::keymgmt::KeyEncryptingKey;
    use closed_sesame::mount::{MountCredentials, mount_vault};
    use std::fs;
    use std::io::Read;
    use std::path::Path;
    use tempfile::TempDir;

    #[cfg(target_os = "linux")]
    use std::os::unix::fs::FileTypeExt;

    fn require_fuse(test_name: &str) -> bool {
        #[cfg(target_os = "linux")]
        {
            let available = Path::new("/dev/fuse")
                .metadata()
                .is_ok_and(|metadata| metadata.file_type().is_char_device());
            if !available {
                eprintln!(
                    "SKIP {test_name}: /dev/fuse is unavailable or is not a character device"
                );
            }
            available
        }
    }

    fn test_mount_point(dir: &TempDir, test_name: &str) -> std::path::PathBuf {
        let _ = test_name;
        dir.path().join("mnt")
    }

    fn prepare_mount_point(mount_point: &Path) {
        if mount_point.exists() {
            let _ = fs::remove_dir_all(mount_point);
        }
        fs::create_dir_all(mount_point).expect("mount point directory");
    }

    fn mount_settle_delay() -> std::time::Duration {
        std::time::Duration::from_millis(200)
    }

    /// Full pipeline: create container → add file → FUSE mount → read via mount point
    #[test]
    fn fuse_mount_read_file() {
        if !require_fuse("fuse_mount_read_file") {
            return;
        }
        let dir = TempDir::new().unwrap();
        let sbox_path = dir.path().join("fuse_test.sesame");
        let mount_point = test_mount_point(&dir, "fuse_mount_read_file");
        let password = "fuse-test-pass";

        // 1. Create container
        let salt = Salt::generate();
        let kek = KeyEncryptingKey::from_password(password, &salt, KdfConfig::default_password())
            .unwrap();
        let mut writer = ContainerWriter::create(&sbox_path, &kek, salt, 5 * 1024 * 1024).unwrap();

        // 2. Add a file
        let test_content = b"Hello from inside the vault! This is secret.\n";
        let src = dir.path().join("secret.txt");
        fs::write(&src, test_content).unwrap();
        writer.add_file(&src, "secret.txt").unwrap();
        drop(writer);

        // 3. Mount via FUSE
        prepare_mount_point(&mount_point);
        let mut mounted =
            mount_vault(Credential::Password(password), &sbox_path, &mount_point).unwrap();

        std::thread::sleep(mount_settle_delay());

        // 4. Read file through mount point
        let mounted_file = mount_point.join("secret.txt");
        assert!(mounted_file.exists(), "mounted file should exist");
        let mut buf = Vec::new();
        fs::File::open(&mounted_file)
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf, test_content, "content should match");

        // 5. Unmount (automatic on drop)
        mounted.unmount().unwrap();
        std::thread::sleep(mount_settle_delay());
        assert!(!mounted_file.exists(), "file should be gone after unmount");
    }

    /// Mount then verify root directory listing.
    #[test]
    fn fuse_mount_list_directory() {
        if !require_fuse("fuse_mount_list_directory") {
            return;
        }
        let dir = TempDir::new().unwrap();
        let sbox_path = dir.path().join("list_test.sesame");
        let mount_point = test_mount_point(&dir, "fuse_mount_list_directory");
        let password = "list-pass";

        let salt = Salt::generate();
        let kek = KeyEncryptingKey::from_password(password, &salt, KdfConfig::default_password())
            .unwrap();
        let mut writer = ContainerWriter::create(&sbox_path, &kek, salt, 5 * 1024 * 1024).unwrap();

        // Add 3 files
        for name in &["alpha.txt", "beta.txt", "gamma.txt"] {
            let src = dir.path().join(name);
            fs::write(&src, format!("content of {name}")).unwrap();
            writer.add_file(&src, name).unwrap();
        }
        drop(writer);

        prepare_mount_point(&mount_point);
        let mut mounted =
            mount_vault(Credential::Password(password), &sbox_path, &mount_point).unwrap();
        std::thread::sleep(mount_settle_delay());

        // List directory
        let entries: Vec<String> = fs::read_dir(&mount_point)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();

        println!("readdir returned: {:?}", entries);
        assert!(
            entries.contains(&"alpha.txt".to_string()),
            "entries: {:?}",
            entries
        );
        assert!(entries.contains(&"beta.txt".to_string()));
        assert!(entries.contains(&"gamma.txt".to_string()));

        mounted.unmount().unwrap();
    }

    /// Mount should fail with wrong password.
    #[test]
    fn fuse_mount_wrong_password() {
        let dir = TempDir::new().unwrap();
        let sbox_path = dir.path().join("wrong.sesame");
        let mount_point = test_mount_point(&dir, "fuse_mount_wrong_password");

        let salt = Salt::generate();
        let kek =
            KeyEncryptingKey::from_password("right-password", &salt, KdfConfig::default_password())
                .unwrap();
        ContainerWriter::create(&sbox_path, &kek, salt, 1024 * 1024).unwrap();

        let result = mount_vault(
            Credential::Password("wrong-password"),
            &sbox_path,
            &mount_point,
        );
        assert!(result.is_err(), "wrong password should fail");
    }

    /// Mount an empty container — should work, directory just empty.
    #[test]
    fn fuse_mount_empty_container() {
        if !require_fuse("fuse_mount_empty_container") {
            return;
        }
        let dir = TempDir::new().unwrap();
        let sbox_path = dir.path().join("empty.sesame");
        let mount_point = test_mount_point(&dir, "fuse_mount_empty_container");
        let salt = Salt::generate();
        let kek = KeyEncryptingKey::no_password(&salt, KdfConfig::no_password()).unwrap();
        ContainerWriter::create(&sbox_path, &kek, salt, 1024 * 1024).unwrap();

        prepare_mount_point(&mount_point);
        let mut mounted =
            mount_vault(MountCredentials::NoPassword, &sbox_path, &mount_point).unwrap();
        std::thread::sleep(mount_settle_delay());

        let count = fs::read_dir(&mount_point).unwrap().count();
        assert_eq!(count, 0, "empty container should have no files");

        mounted.unmount().unwrap();
    }

    /// Write path: create a new file through the mount and verify it lands
    /// in the container after unmount.
    #[test]
    fn fuse_mount_write_new_file() {
        if !require_fuse("fuse_mount_write_new_file") {
            return;
        }
        let dir = TempDir::new().unwrap();
        let sbox_path = dir.path().join("write_new.sesame");
        let mount_point = test_mount_point(&dir, "fuse_mount_write_new_file");
        let password = "write-new-pass";
        let salt = Salt::generate();
        let kek = KeyEncryptingKey::from_password(password, &salt, KdfConfig::default_password())
            .unwrap();
        ContainerWriter::create(&sbox_path, &kek, salt, 5 * 1024 * 1024).unwrap();

        prepare_mount_point(&mount_point);
        let mut mounted =
            mount_vault(Credential::Password(password), &sbox_path, &mount_point).unwrap();
        std::thread::sleep(mount_settle_delay());

        let content = b"created through the writable mount\n";
        fs::write(mount_point.join("created.txt"), content).unwrap();
        // Visible through the mount immediately.
        assert_eq!(fs::read(mount_point.join("created.txt")).unwrap(), content);
        mounted.unmount().unwrap();
        std::thread::sleep(mount_settle_delay());

        // Persisted inside the container.
        let reader = closed_sesame::ContainerReader::open(&sbox_path, &kek).unwrap();
        let entries = reader.list_files().unwrap();
        let entry = entries
            .iter()
            .find(|entry| entry.path == "created.txt")
            .expect("created.txt must exist in the container");
        assert_eq!(reader.read_file(entry).unwrap(), content);
    }

    /// Write path: overwrite an existing file through the mount; the old
    /// content must be fully replaced, never corrupted or duplicated.
    #[test]
    fn fuse_mount_edit_existing_file() {
        if !require_fuse("fuse_mount_edit_existing_file") {
            return;
        }
        let dir = TempDir::new().unwrap();
        let sbox_path = dir.path().join("edit.sesame");
        let mount_point = test_mount_point(&dir, "fuse_mount_edit_existing_file");
        let password = "edit-pass";
        let salt = Salt::generate();
        let kek = KeyEncryptingKey::from_password(password, &salt, KdfConfig::default_password())
            .unwrap();
        let mut writer = ContainerWriter::create(&sbox_path, &kek, salt, 5 * 1024 * 1024).unwrap();
        let src = dir.path().join("doc.txt");
        fs::write(&src, b"original content that is quite long\n").unwrap();
        writer.add_file(&src, "doc.txt").unwrap();
        drop(writer);

        prepare_mount_point(&mount_point);
        let mut mounted =
            mount_vault(Credential::Password(password), &sbox_path, &mount_point).unwrap();
        std::thread::sleep(mount_settle_delay());

        // Shorter replacement also truncates (O_TRUNC on open).
        let replacement = b"short\n";
        fs::write(mount_point.join("doc.txt"), replacement).unwrap();
        mounted.unmount().unwrap();
        std::thread::sleep(mount_settle_delay());

        let reader = closed_sesame::ContainerReader::open(&sbox_path, &kek).unwrap();
        let entries = reader.list_files().unwrap();
        let matching: Vec<_> = entries
            .iter()
            .filter(|entry| entry.path == "doc.txt")
            .collect();
        assert_eq!(matching.len(), 1, "exactly one doc.txt entry");
        assert_eq!(reader.read_file(matching[0]).unwrap(), replacement);
    }

    /// Write path: mkdir, nested write, rename, unlink, rmdir through the
    /// mount, verified against the container afterwards.
    #[test]
    fn fuse_mount_mkdir_rename_unlink() {
        if !require_fuse("fuse_mount_mkdir_rename_unlink") {
            return;
        }
        let dir = TempDir::new().unwrap();
        let sbox_path = dir.path().join("tree.sesame");
        let mount_point = test_mount_point(&dir, "fuse_mount_mkdir_rename_unlink");
        let password = "tree-pass";
        let salt = Salt::generate();
        let kek = KeyEncryptingKey::from_password(password, &salt, KdfConfig::default_password())
            .unwrap();
        ContainerWriter::create(&sbox_path, &kek, salt, 5 * 1024 * 1024).unwrap();

        prepare_mount_point(&mount_point);
        let mut mounted =
            mount_vault(Credential::Password(password), &sbox_path, &mount_point).unwrap();
        std::thread::sleep(mount_settle_delay());

        fs::create_dir(mount_point.join("docs")).unwrap();
        fs::write(mount_point.join("docs/a.txt"), b"nested\n").unwrap();
        fs::rename(
            mount_point.join("docs/a.txt"),
            mount_point.join("docs/b.txt"),
        )
        .unwrap();
        assert!(mount_point.join("docs/b.txt").exists());
        fs::remove_file(mount_point.join("docs/b.txt")).unwrap();
        fs::remove_dir(mount_point.join("docs")).unwrap();
        mounted.unmount().unwrap();
        std::thread::sleep(mount_settle_delay());

        let reader = closed_sesame::ContainerReader::open(&sbox_path, &kek).unwrap();
        let entries = reader.list_files().unwrap();
        assert!(
            entries.is_empty(),
            "container should be empty, found: {:?}",
            entries.iter().map(|entry| &entry.path).collect::<Vec<_>>()
        );
    }
}
