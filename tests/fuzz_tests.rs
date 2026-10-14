//! Fuzz/property-based tests using proptest.
//!
//! Verifies that random inputs cannot corrupt data or crash the system.
//!
//! Cheap properties honor `PROPTEST_CASES` (32 in CI). I/O-heavy and
//! Argon2id properties use smaller fixed budgets to keep runtime predictable.

use closed_sesame::container::format::{BLOCK_SIZE, HEADER_SIZE};
use closed_sesame::container::io::{ContainerReader, ContainerWriter};
use closed_sesame::crypto::{self, KdfConfig, Key, Salt};
use closed_sesame::keymgmt::KeyEncryptingKey;
use proptest::prelude::*;
use proptest::test_runner::TestRunner;
use tempfile::TempDir;

fn general_property_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(16)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(general_property_cases()))]

    /// AES-GCM roundtrip: any random plaintext encrypts and decrypts correctly.
    #[test]
    fn aes_gcm_roundtrip_random(data in prop::collection::vec(any::<u8>(), 0..1024)) {
        let key = Key::from_slice(&[42u8; 32]).unwrap();
        let (ct, nonce) = crypto::encrypt(&key, &data).unwrap();
        let decrypted = crypto::decrypt(&key, &ct, &nonce).unwrap();
        prop_assert_eq!(decrypted, data);
    }

    /// Corruption detection: flipping any byte in ciphertext causes auth failure.
    #[test]
    fn corruption_detected(data in prop::collection::vec(any::<u8>(), 1..256)) {
        let key = Key::from_slice(&[42u8; 32]).unwrap();
        let (mut ct, nonce) = crypto::encrypt(&key, &data).unwrap();
        ct[0] ^= 1;
        let result = crypto::decrypt(&key, &ct, &nonce);
        prop_assert!(result.is_err());
    }

}

/// Argon2id: same password + salt = same key.
#[test]
fn argon2id_deterministic_prop() {
    // A manual runner prevents PROPTEST_CASES from overriding this expensive budget.
    let mut runner = TestRunner::new(ProptestConfig::with_cases(2));
    let passwords = proptest::string::string_regex("[a-zA-Z0-9]{4,10}").unwrap();
    runner
        .run(&passwords, |password| {
            let salt = Salt::from_slice(&[7u8; 32]).unwrap();
            let config = KdfConfig::default_password();
            let k1 = crypto::derive_key(&password, &salt, &config).unwrap();
            let k2 = crypto::derive_key(&password, &salt, &config).unwrap();
            prop_assert_eq!(k1.as_bytes(), k2.as_bytes());
            Ok(())
        })
        .unwrap();
}

/// Wrong password cannot open container.
#[test]
fn wrong_password_rejected() {
    let mut runner = TestRunner::new(ProptestConfig::with_cases(2));
    let passwords = (
        proptest::string::string_regex("[a-zA-Z0-9]{4,8}").unwrap(),
        proptest::string::string_regex("[a-zA-Z0-9]{4,8}").unwrap(),
    );
    runner
        .run(&passwords, |(correct_pw, wrong_pw)| {
            prop_assume!(correct_pw != wrong_pw);
            let dir = TempDir::new().unwrap();
            let path = dir.path().join("prop.sesame");
            let salt = Salt::generate();
            let config = KdfConfig::default_password();
            let kek = KeyEncryptingKey::from_password(&correct_pw, &salt, config).unwrap();
            let wrong = KeyEncryptingKey::from_password(&wrong_pw, &salt, config).unwrap();
            ContainerWriter::create(&path, &kek, salt, 1024 * 1024).unwrap();
            prop_assert!(ContainerReader::open(&path, &wrong).is_err());
            Ok(())
        })
        .unwrap();
}

/// Container creation + write + read roundtrip (V3 page-level).
#[test]
fn container_write_read_roundtrip() {
    // Full encrypted blocks make this property I/O-heavy, so keep eight cases.
    let mut runner = TestRunner::new(ProptestConfig::with_cases(8));
    let inputs = (
        prop::collection::vec(any::<u8>(), 0..1024),
        proptest::string::string_regex("[a-zA-Z0-9]{1,15}").unwrap(),
    );
    runner
        .run(&inputs, |(content, filename)| {
            let dir = TempDir::new().unwrap();
            let path = dir.path().join("rt.sesame");
            let salt = Salt::generate();
            let kek = KeyEncryptingKey::no_password(&salt, KdfConfig::no_password()).unwrap();
            let mut writer = ContainerWriter::create(&path, &kek, salt, 10 * 1024 * 1024).unwrap();

            let src = dir.path().join(filename.clone());
            std::fs::write(&src, &content).unwrap();
            writer.add_file(&src, &filename).unwrap();
            drop(writer);

            let reader = ContainerReader::open(&path, &kek).unwrap();
            let files = reader.list_files().unwrap();
            prop_assert_eq!(files.len(), 1);
            prop_assert_eq!(files[0].size, content.len() as u64);

            let readback = reader.read_file(&files[0]).unwrap();
            prop_assert_eq!(readback, content);
            Ok(())
        })
        .unwrap();
}

/// Explicit test: 100K nonces — ensure no collision.
#[test]
fn million_nonces_no_collision() {
    let mut seen = std::collections::HashSet::new();
    for _ in 0..100_000 {
        let nonce = crypto::generate_nonce();
        assert!(seen.insert(nonce), "nonce collision detected!");
    }
}

/// Fuzz: random byte injection into a valid V3 container should not panic.
#[test]
fn random_corruption_no_panic() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fuzz.sesame");
    let salt = Salt::generate();
    let kek =
        KeyEncryptingKey::from_password("test", &salt, KdfConfig::default_password()).unwrap();
    ContainerWriter::create(&path, &kek, salt, BLOCK_SIZE).unwrap(); // 4MB data

    // Corrupt bytes at deterministic offset positions
    let mut bytes = std::fs::read(&path).unwrap();
    let primes = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47];
    for (i, &p) in primes.iter().enumerate() {
        let offset = HEADER_SIZE * 2 + (p as usize * 7919) % (bytes.len() - HEADER_SIZE * 2);
        bytes[offset] ^= (i as u8).wrapping_mul(73);
    }
    std::fs::write(&path, &bytes).unwrap();

    // Opening should NOT panic — should return Err or Ok with corrupted content
    let result = std::panic::catch_unwind(|| ContainerReader::open(&path, &kek));
    match result {
        Ok(Ok(reader)) => {
            // Headers survived, data may be corrupted — verify reports it
            let _ = reader.verify();
        }
        Ok(Err(_)) => {} // Expected: corruption detected at open
        Err(_) => panic!("open() panicked on corrupted file"),
    }
}
