//! Best-effort screenshot/recording deterrence for the GUI window.

/// Apply anti-capture via egui viewport command (winit `set_content_protected`).
pub fn apply(ctx: &egui::Context, enabled: bool, last: &mut Option<bool>) {
    if *last == Some(enabled) {
        return;
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::ContentProtected(enabled));
    *last = Some(enabled);
}

pub fn platform_support_note() -> String {
    #[cfg(windows)]
    {
        ts!("anti_capture.windows")
    }
    #[cfg(target_os = "macos")]
    {
        ts!("anti_capture.macos")
    }
    #[cfg(target_os = "linux")]
    {
        linux_support_note()
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        ts!("anti_capture.unsupported")
    }
}

#[cfg(target_os = "linux")]
fn linux_support_note() -> String {
    if is_wayland() {
        if is_kde_plasma() {
            ts!("anti_capture.linux_kde_wayland")
        } else {
            ts!("anti_capture.linux_wayland")
        }
    } else {
        ts!("anti_capture.linux_x11")
    }
}

#[cfg(target_os = "linux")]
fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v.eq_ignore_ascii_case("wayland"))
}

#[cfg(target_os = "linux")]
fn is_kde_plasma() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|v| v.to_ascii_lowercase().contains("kde"))
        || std::env::var_os("KDE_SESSION_VERSION").is_some()
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use super::{is_kde_plasma, is_wayland};

    #[cfg(target_os = "linux")]
    #[test]
    fn wayland_detection_reads_session_type() {
        // SAFETY: tests run single-threaded; env mutations are isolated
        let prev = std::env::var_os("WAYLAND_DISPLAY");
        unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
        unsafe { std::env::set_var("XDG_SESSION_TYPE", "wayland") };
        assert!(is_wayland());
        if let Some(value) = prev {
            unsafe { std::env::set_var("WAYLAND_DISPLAY", value) };
        } else {
            unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
        }
        unsafe { std::env::remove_var("XDG_SESSION_TYPE") };
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn kde_detection_reads_desktop_env() {
        let prev = std::env::var("XDG_CURRENT_DESKTOP").ok();
        unsafe { std::env::set_var("XDG_CURRENT_DESKTOP", "KDE") };
        assert!(is_kde_plasma());
        match prev {
            Some(value) => unsafe { std::env::set_var("XDG_CURRENT_DESKTOP", value) },
            None => unsafe { std::env::remove_var("XDG_CURRENT_DESKTOP") },
        }
    }
}
