// Do not spawn a console window alongside the GUI on Windows.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

rust_i18n::i18n!("locales", fallback = "en");

fn main() -> eframe::Result<()> {
    closed_sesame::gui::run()
}
