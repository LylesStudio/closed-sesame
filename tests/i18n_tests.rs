use closed_sesame::SesameError;
use closed_sesame::i18n::{self, Locale, keys};

fn keys_in_locale(path: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for line in std::fs::read_to_string(path).unwrap().lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('_') {
            continue;
        }
        let Some((key, _)) = line.split_once(':') else {
            continue;
        };
        keys.push(key.trim().to_owned());
    }
    keys.sort();
    keys.dedup();
    keys
}

#[test]
fn locale_files_have_identical_key_sets() {
    let en = keys_in_locale("locales/en.yml");
    let zh = keys_in_locale("locales/zh-CN.yml");
    let ja = keys_in_locale("locales/ja.yml");
    assert!(!en.is_empty());
    assert_eq!(en, zh);
    assert_eq!(en, ja);
}

#[test]
fn localized_errors_cover_key_variants() {
    for locale in Locale::ALL {
        i18n::init(Some(locale));
        assert!(!SesameError::WrongPassword.localized().is_empty());
        assert!(
            !SesameError::InvalidInput(keys::PATH_EMPTY.into())
                .localized()
                .is_empty()
        );
        assert!(
            !SesameError::InvalidInput(keys::SESAME_PATH_REQUIRED.into())
                .localized()
                .is_empty()
        );
        assert!(
            !SesameError::ExtractConflict("out/doc.txt".into())
                .localized()
                .is_empty()
        );
        assert!(
            !SesameError::ExtractMountReadOnly("Z:\\out".into())
                .localized()
                .is_empty()
        );
    }
}

#[test]
fn mount_hints_localized_for_each_locale() {
    use closed_sesame::mount_hints::platform_prerequisite_hint;
    for locale in Locale::ALL {
        i18n::init(Some(locale));
        assert!(!platform_prerequisite_hint().is_empty());
    }
}
