use crate::error::{Result, SesameError};
use crate::i18n::keys;
use std::path::PathBuf;

pub fn parse_size(value: &str) -> Result<usize> {
    let value = value.trim().to_ascii_uppercase();
    let split = value
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(value.len());
    let number: f64 = value[..split].parse().map_err(|_| {
        SesameError::InvalidInput(format!("{}:{}", keys::INVALID_SIZE, &value[..split]))
    })?;
    let multiplier = match &value[split..] {
        "" | "B" => 1.0,
        "KB" => 1024.0,
        "MB" => 1024.0 * 1024.0,
        "GB" => 1024.0 * 1024.0 * 1024.0,
        unit => {
            return Err(SesameError::InvalidInput(format!(
                "{}:{unit}",
                keys::UNKNOWN_UNIT
            )));
        }
    };
    if !number.is_finite() || number < 0.0 || number * multiplier > usize::MAX as f64 {
        return Err(SesameError::InvalidInput(keys::SIZE_OUT_OF_RANGE.into()));
    }
    Ok((number * multiplier) as usize)
}

pub fn fmt_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1}GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.0}MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0}KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes}B")
    }
}

pub fn ensure_sesame_extension(path: impl Into<PathBuf>) -> PathBuf {
    let path = path.into();
    if path
        .extension()
        .is_some_and(|extension| extension == "sesame")
    {
        path
    } else {
        path.with_extension("sesame")
    }
}
