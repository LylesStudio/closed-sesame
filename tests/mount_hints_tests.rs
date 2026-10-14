#![cfg(feature = "mount")]

use closed_sesame::SesameError;
use closed_sesame::i18n::{Locale, init};
#[cfg(target_os = "windows")]
use closed_sesame::mount_hints::WINFSP_RELEASES_URL;
use closed_sesame::mount_hints::supplemental_guidance;

#[test]
fn platform_error_includes_runtime_guidance() {
    init(Some(Locale::En));
    let error = SesameError::Platform("WinFsp runtime is required".into());
    let guidance = supplemental_guidance(&error).expect("guidance");
    assert!(!guidance.is_empty());
    #[cfg(target_os = "windows")]
    assert!(guidance.contains(WINFSP_RELEASES_URL));
}
