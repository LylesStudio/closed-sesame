//! Open decrypted files with the system default application.

use crate::error::{Result, SesameError};
use std::path::Path;
use std::process::Command;

pub fn open_path(path: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: the GUI binary has no console, so without this
        // flag `cmd` flashes a black console window every time a file opens.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new("cmd")
            .args(["/C", "start", "", &path.display().to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| SesameError::Platform(format!("cannot open file: {error}")))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|error| SesameError::Platform(format!("cannot open file: {error}")))?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| SesameError::Platform(format!("cannot open file: {error}")))?;
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = path;
        return Err(SesameError::Platform(
            "open_external is unsupported on this platform".into(),
        ));
    }
    Ok(())
}
