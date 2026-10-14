use super::app::App;
use crate::i18n::Locale;
use std::path::PathBuf;

mod browser;
mod locale_setup;
mod locked;
mod settings;

pub(crate) enum Event {
    BrowseVault,
    OpenRecent(String),
    Open,
    Create,
    SetNoPassword(bool),
    AddFiles,
    AddFolder,
    AddPaths(Vec<PathBuf>),
    Refresh,
    Lock,
    EnterDirectory(String),
    SelectEntry {
        path: String,
        ctrl: bool,
        shift: bool,
        /// Paths in the order the user currently sees them (sorted/filtered),
        /// so shift-range selection follows the visible list.
        ordered: Vec<String>,
    },
    Mkdir,
    RemoveSelected,
    /// Start inline rename of the single selected entry.
    BeginRename,
    CommitRename,
    CancelRename,
    SelectAll,
    ClearSelection,
    GoUp,
    ExtractSelected,
    BeginCopy,
    BeginMove,
    BeginMoveOut,
    OpenSelected,
    #[cfg(feature = "mount")]
    Mount,
    #[cfg(feature = "mount")]
    Unmount,
    #[cfg(feature = "mount")]
    SetCreateSymlinks(bool),
    ToggleTheme,
    SetTheme(crate::gui::theme::Theme),
    ShowSettings,
    Back,
    SetAutoLockMinutes(u32),
    SetAutoLockEnabled(bool),
    SetAntiCapture(bool),
    VerifyVault,
    DefragVault,
    RecoverVault,
    BeginChangePassword,
    BeginGrow,
    SetLocale(Locale),
    ToggleNotifications,
    ClearNotifications,
    #[cfg(feature = "mount")]
    OpenMountLocation,
}

pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) -> Vec<Event> {
    let mut events = Vec::new();
    if app.needs_locale_setup() {
        locale_setup::show(app, ui, &mut events);
    } else if app.show_settings {
        settings::show(app, ui, &mut events);
    } else if app.is_unlocked() {
        browser::show(app, ui, &mut events);
    } else {
        locked::show(app, ui, &mut events);
    }
    events
}
