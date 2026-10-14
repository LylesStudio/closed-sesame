//! Normalized in-container path handling (`/` separators, no `..`).

use crate::error::{Result, SesameError};
use crate::i18n::keys;

pub const MAX_COMPONENT_LEN: usize = 255;

/// Normalize user input into a canonical container path.
pub fn normalize(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(SesameError::InvalidInput(keys::PATH_EMPTY.into()));
    }
    if trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return Err(SesameError::InvalidInput(keys::ABSOLUTE_PATH.into()));
    }

    let mut parts = Vec::new();
    for segment in trimmed.split(['/', '\\']) {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(SesameError::InvalidInput(keys::PARENT_SEGMENTS.into()));
        }
        if segment.len() > MAX_COMPONENT_LEN {
            return Err(SesameError::InvalidInput(format!(
                "{}:{segment}",
                keys::PATH_COMPONENT_TOO_LONG
            )));
        }
        parts.push(segment);
    }

    if parts.is_empty() {
        return Err(SesameError::InvalidInput(keys::PATH_EMPTY.into()));
    }

    Ok(parts.join("/"))
}

pub fn parent(path: &str) -> Option<&str> {
    path.rfind('/').map(|index| &path[..index])
}

pub fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn components(path: &str) -> impl Iterator<Item = &str> {
    path.split('/').filter(|segment| !segment.is_empty())
}

pub fn is_prefix(prefix: &str, path: &str) -> bool {
    if prefix == path {
        return true;
    }
    path.starts_with(prefix)
        && path
            .as_bytes()
            .get(prefix.len())
            .is_some_and(|byte| *byte == b'/')
}

pub fn join(parent: &str, child: &str) -> Result<String> {
    normalize(&format!("{parent}/{child}"))
}

pub fn ancestors(path: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = path.to_owned();
    while let Some(parent) = parent(&current) {
        if !parent.is_empty() {
            result.push(parent.to_owned());
        }
        current = parent.to_owned();
    }
    result.reverse();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_slashes_and_rejects_invalid_paths() {
        assert_eq!(normalize("a/b/c").unwrap(), "a/b/c");
        assert_eq!(normalize("a\\b/c").unwrap(), "a/b/c");
        assert_eq!(normalize("./a//b/.").unwrap(), "a/b");
        assert!(normalize("").is_err());
        assert!(normalize("/abs").is_err());
        assert!(normalize("a/../b").is_err());
        assert!(normalize(".").is_err());
    }

    #[test]
    fn parent_and_prefix_helpers_work() {
        assert_eq!(parent("a/b/c"), Some("a/b"));
        assert_eq!(file_name("a/b/c"), "c");
        assert!(is_prefix("a", "a/b"));
        assert!(!is_prefix("a/b", "a"));
        assert_eq!(ancestors("a/b/c"), vec!["a", "a/b"]);
    }
}
