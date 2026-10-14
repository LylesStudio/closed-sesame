//! Centralized internationalization for ClosedSesame.

pub use rust_i18n::t;

/// Stable keys stored in [`crate::error::SesameError::InvalidInput`] for localization.
pub mod keys {
    pub const PATH_EMPTY: &str = "error.invalid_input.path_empty";
    pub const ABSOLUTE_PATH: &str = "error.invalid_input.absolute_path";
    pub const PARENT_SEGMENTS: &str = "error.invalid_input.parent_segments";
    pub const PATH_COMPONENT_TOO_LONG: &str = "error.invalid_input.path_component_too_long";
    pub const INVALID_SIZE: &str = "error.invalid_input.invalid_size";
    pub const UNKNOWN_UNIT: &str = "error.invalid_input.unknown_unit";
    pub const SIZE_OUT_OF_RANGE: &str = "error.invalid_input.size_out_of_range";
    pub const PASSWORD_EMPTY: &str = "error.invalid_input.password_empty";
    pub const PASSWORD_MISMATCH: &str = "error.invalid_input.password_mismatch";
    pub const COPY_INTO_SELF: &str = "error.invalid_input.copy_into_self";
    pub const DESTINATION_EXISTS: &str = "error.invalid_input.destination_exists";
    pub const PASSWORD_KDF: &str = "error.invalid_input.password_kdf";
    pub const NO_PASSWORD_KDF: &str = "error.invalid_input.no_password_kdf";
    pub const SESAME_PATH_REQUIRED: &str = "error.invalid_input.sesame_path_required";
    pub const MOUNT_POINT_REQUIRED_SYMLINK: &str =
        "error.invalid_input.mount_point_required_symlink";
    pub const MOUNT_POINT_BUSY: &str = "error.mount_point_busy";
    pub const RECURSIVE_REQUIRES_DIR: &str = "error.invalid_input.recursive_requires_dir";
    pub const CANNOT_READ_ROOT_AS_FILE: &str = "error.invalid_input.cannot_read_root_as_file";
    pub const EMPTY_PATH: &str = "error.gui.empty_path";
    pub const INVALID_TARGET_PATH: &str = "error.gui.invalid_target_path";
    pub const CANNOT_INSPECT_PATH: &str = "error.gui.cannot_inspect_path";
    pub const FILE_NOT_READABLE: &str = "error.gui.file_not_readable";
    pub const ITEMS_COUNT: &str = "error.gui.items_count";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    En,
    ZhCn,
    Ja,
}

impl Locale {
    pub const ALL: [Self; 3] = [Self::En, Self::ZhCn, Self::Ja];

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhCn => "zh-CN",
            Self::Ja => "ja",
        }
    }

    pub fn label(self) -> String {
        ts!(match self {
            Self::En => "settings.locale.en",
            Self::ZhCn => "settings.locale.zh_cn",
            Self::Ja => "settings.locale.ja",
        })
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" | "en-US" | "en-GB" => Some(Self::En),
            "zh" | "zh-CN" | "zh-Hans" | "zh-Hans-CN" => Some(Self::ZhCn),
            "ja" | "ja-JP" => Some(Self::Ja),
            _ => None,
        }
    }

    pub fn from_env() -> Option<Self> {
        std::env::var("CLOSED_SESAME_LANG")
            .ok()
            .and_then(|value| Self::from_code(value.trim()))
    }

    pub fn from_system() -> Option<Self> {
        sys_locale::get_locale().and_then(|value| Self::from_code(&value))
    }
}

/// Initialize locale from explicit choice, environment, or system default.
pub fn init(locale: Option<Locale>) {
    let locale = locale
        .or_else(Locale::from_env)
        .or_else(Locale::from_system)
        .unwrap_or(Locale::En);
    rust_i18n::set_locale(locale.code());
}

/// Resolve a stored invalid-input key or fall back to the raw message.
pub fn localized_invalid_input(message: &str) -> String {
    use keys::*;
    match message {
        PATH_EMPTY => ts!("error.invalid_input.path_empty"),
        ABSOLUTE_PATH => ts!("error.invalid_input.absolute_path"),
        PARENT_SEGMENTS => ts!("error.invalid_input.parent_segments"),
        PATH_COMPONENT_TOO_LONG => ts!("error.invalid_input.path_component_too_long"),
        INVALID_SIZE => ts!("error.invalid_input.invalid_size"),
        UNKNOWN_UNIT => ts!("error.invalid_input.unknown_unit"),
        SIZE_OUT_OF_RANGE => ts!("error.invalid_input.size_out_of_range"),
        PASSWORD_EMPTY => ts!("error.invalid_input.password_empty"),
        PASSWORD_MISMATCH => ts!("error.invalid_input.password_mismatch"),
        COPY_INTO_SELF => ts!("error.invalid_input.copy_into_self"),
        DESTINATION_EXISTS => ts!("error.invalid_input.destination_exists"),
        PASSWORD_KDF => ts!("error.invalid_input.password_kdf"),
        NO_PASSWORD_KDF => ts!("error.invalid_input.no_password_kdf"),
        SESAME_PATH_REQUIRED => ts!("error.invalid_input.sesame_path_required"),
        MOUNT_POINT_REQUIRED_SYMLINK => ts!("error.invalid_input.mount_point_required_symlink"),
        RECURSIVE_REQUIRES_DIR => ts!("error.invalid_input.recursive_requires_dir"),
        CANNOT_READ_ROOT_AS_FILE => ts!("error.invalid_input.cannot_read_root_as_file"),
        EMPTY_PATH => ts!("error.gui.empty_path"),
        INVALID_TARGET_PATH => ts!("error.gui.invalid_target_path"),
        other if other.starts_with(&format!("{INVALID_SIZE}:")) => {
            let number = other
                .strip_prefix(&format!("{INVALID_SIZE}:"))
                .unwrap_or(other);
            ts!(
                "error.invalid_input.invalid_size_with_value",
                number = number
            )
        }
        other if other.starts_with(&format!("{UNKNOWN_UNIT}:")) => {
            let unit = other
                .strip_prefix(&format!("{UNKNOWN_UNIT}:"))
                .unwrap_or(other);
            ts!("error.invalid_input.unknown_unit_with_value", unit = unit)
        }
        other if other.starts_with(&format!("{PATH_COMPONENT_TOO_LONG}:")) => {
            let segment = other
                .strip_prefix(&format!("{PATH_COMPONENT_TOO_LONG}:"))
                .unwrap_or(other);
            ts!(
                "error.invalid_input.path_component_too_long_with_segment",
                max = crate::container::path::MAX_COMPONENT_LEN,
                segment = segment
            )
        }
        other if other.starts_with(&format!("{CANNOT_INSPECT_PATH}:")) => {
            let detail = other
                .strip_prefix(&format!("{CANNOT_INSPECT_PATH}:"))
                .unwrap_or(other);
            ts!("error.gui.cannot_inspect_path", detail = detail)
        }
        other if other.starts_with(&format!("{FILE_NOT_READABLE}:")) => {
            let detail = other
                .strip_prefix(&format!("{FILE_NOT_READABLE}:"))
                .unwrap_or(other);
            ts!("error.gui.file_not_readable", detail = detail)
        }
        other if other.starts_with(&format!("{ITEMS_COUNT}:")) => {
            let count = other
                .strip_prefix(&format!("{ITEMS_COUNT}:"))
                .unwrap_or(other);
            ts!("error.gui.items_count", count = count)
        }
        other if other.starts_with(&format!("{MOUNT_POINT_BUSY}:")) => {
            let path = other
                .strip_prefix(&format!("{MOUNT_POINT_BUSY}:"))
                .unwrap_or(other);
            ts!("error.mount_point_busy", path = path)
        }
        other => format!("{}: {other}", ts!("error.invalid_input.prefix")),
    }
}

/// Localize GUI action rejection reasons and stored error strings.
pub fn localized_message(message: &str) -> String {
    localized_invalid_input(message)
}

#[cfg(any(feature = "cli", feature = "recover"))]
pub fn extract_lang_from_args() -> Option<Locale> {
    let args: Vec<String> = std::env::args().collect();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--lang" => {
                return args
                    .get(index + 1)
                    .and_then(|value| Locale::from_code(value.trim()));
            }
            value => {
                if let Some(code) = value.strip_prefix("--lang=") {
                    return Locale::from_code(code.trim());
                }
            }
        }
        index += 1;
    }
    None
}

#[cfg(any(feature = "cli", feature = "recover"))]
pub fn init_cli() {
    init(extract_lang_from_args());
}
