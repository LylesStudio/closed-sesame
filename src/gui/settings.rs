//! Persistent GUI settings (locale, etc.).

use super::theme::Theme;
use crate::i18n::Locale;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const FILE_NAME: &str = "settings.json";

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HintPreferences {
    #[serde(default)]
    pub skip_extract_info: bool,
    #[serde(default)]
    pub skip_symlink_info: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiSettings {
    #[serde(default)]
    pub locale: Option<LocaleCode>,
    #[serde(default)]
    pub hints: HintPreferences,
    #[serde(default = "default_true")]
    pub anti_capture_enabled: bool,
    /// Retained so older settings files still parse. Extract always keeps
    /// the vault copy; moving out is a separate confirmed action.
    #[serde(default = "default_true")]
    pub keep_files_on_extract: bool,
    #[serde(default)]
    pub theme: Theme,
    #[serde(default = "default_true")]
    pub auto_lock_enabled: bool,
    #[serde(default = "default_auto_lock_minutes")]
    pub auto_lock_minutes: u32,
}

fn default_auto_lock_minutes() -> u32 {
    10
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LocaleCode {
    En,
    ZhCn,
    Ja,
}

impl LocaleCode {
    pub fn to_locale(self) -> Locale {
        match self {
            Self::En => Locale::En,
            Self::ZhCn => Locale::ZhCn,
            Self::Ja => Locale::Ja,
        }
    }

    pub fn from_locale(locale: Locale) -> Self {
        match locale {
            Locale::En => Self::En,
            Locale::ZhCn => Self::ZhCn,
            Locale::Ja => Self::Ja,
        }
    }
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            locale: None,
            hints: HintPreferences::default(),
            anti_capture_enabled: true,
            keep_files_on_extract: true,
            theme: Theme::default(),
            auto_lock_enabled: true,
            auto_lock_minutes: default_auto_lock_minutes(),
        }
    }
}

impl GuiSettings {
    pub fn load() -> Self {
        config_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(path) = config_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    pub fn locale(&self) -> Option<Locale> {
        self.locale.map(LocaleCode::to_locale)
    }

    pub fn set_locale(&mut self, locale: Locale) {
        self.locale = Some(LocaleCode::from_locale(locale));
        self.save();
    }

    pub fn set_skip_extract_info(&mut self, skip: bool) {
        self.hints.skip_extract_info = skip;
        self.save();
    }

    pub fn set_skip_symlink_info(&mut self, skip: bool) {
        self.hints.skip_symlink_info = skip;
        self.save();
    }

    pub fn set_anti_capture(&mut self, enabled: bool) {
        self.anti_capture_enabled = enabled;
        self.save();
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.save();
    }

    pub fn set_auto_lock(&mut self, enabled: bool, minutes: u32) {
        self.auto_lock_enabled = enabled;
        self.auto_lock_minutes = minutes.clamp(1, 60);
        self.save();
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("closed-sesame").join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_preferences_default_false() {
        let settings = GuiSettings::default();
        assert!(!settings.hints.skip_extract_info);
        assert!(!settings.hints.skip_symlink_info);
    }

    #[test]
    fn hint_preferences_deserialize_missing_fields() {
        let settings: GuiSettings = serde_json::from_str(r#"{"locale":"en"}"#).unwrap();
        assert!(!settings.hints.skip_extract_info);
        assert!(!settings.hints.skip_symlink_info);
        assert!(settings.anti_capture_enabled);
    }

    #[test]
    fn anti_capture_deserialize_explicit_false() {
        let settings: GuiSettings =
            serde_json::from_str(r#"{"anti_capture_enabled":false}"#).unwrap();
        assert!(!settings.anti_capture_enabled);
    }

    #[test]
    fn keep_files_on_extract_defaults_to_copy() {
        let settings = GuiSettings::default();
        assert!(settings.keep_files_on_extract);
        let settings: GuiSettings = serde_json::from_str(r#"{"locale":"en"}"#).unwrap();
        assert!(settings.keep_files_on_extract);
        let settings: GuiSettings =
            serde_json::from_str(r#"{"keep_files_on_extract":false}"#).unwrap();
        assert!(!settings.keep_files_on_extract);
    }

    #[test]
    fn theme_and_auto_lock_round_trip_with_defaults() {
        let settings: GuiSettings = serde_json::from_str(r#"{"locale":"en"}"#).unwrap();
        assert_eq!(settings.theme, Theme::Cave);
        assert!(settings.auto_lock_enabled);
        assert_eq!(settings.auto_lock_minutes, 10);
        let settings: GuiSettings = serde_json::from_str(
            r#"{"theme":"parchment","auto_lock_enabled":false,"auto_lock_minutes":25}"#,
        )
        .unwrap();
        assert_eq!(settings.theme, Theme::Parchment);
        assert!(!settings.auto_lock_enabled);
        assert_eq!(settings.auto_lock_minutes, 25);
    }
}
