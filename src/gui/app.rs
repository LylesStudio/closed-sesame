use super::actions::{self, Capacity, FileEntryInfo, Session};
use super::anti_capture;
use super::auto_lock::AutoLock;
use super::browser::BrowserTree;
use super::cache::{self, CacheSession, ConflictResolution, WritebackPlan};
use super::dialog::{ActiveDialog, DialogOutcome};
use super::job::{self, JobOutput};
use super::recent::RecentStore;
use super::settings::GuiSettings;
use super::theme::{Palette, Theme};
use super::views::{self, Event};
use crate::app::parse_size;
#[cfg(feature = "mount")]
use crate::app::vault;
use crate::container::crc32;
use crate::i18n::{Locale, localized_message};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use zeroize::Zeroize;

/// File list ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortKey {
    Name,
    Size,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sort {
    pub(crate) key: SortKey,
    pub(crate) ascending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Self {
            key: SortKey::Name,
            ascending: true,
        }
    }
}

/// Widgets the browser view can be asked to focus on the next frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusTarget {
    Rename,
    Mkdir,
    Search,
}

impl Sort {
    /// Click on a column header: toggle direction if same key, else switch.
    pub(crate) fn toggle(&mut self, key: SortKey) {
        if self.key == key {
            self.ascending = !self.ascending;
        } else {
            self.key = key;
            self.ascending = key == SortKey::Name;
        }
    }
}

pub(crate) struct App {
    pub(crate) theme: Theme,
    /// Colors painted this frame (cross-faded between themes).
    pub(crate) palette: Palette,
    pub(crate) locale: Locale,
    pub(crate) settings: GuiSettings,
    pub(crate) path_input: String,
    pub(crate) password: String,
    pub(crate) no_password: bool,
    pub(crate) status: String,
    pub(crate) entries: Vec<FileEntryInfo>,
    pub(crate) browser: BrowserTree,
    pub(crate) current_dir: String,
    pub(crate) selected_paths: BTreeSet<String>,
    pub(crate) selection_anchor: Option<String>,
    /// Inline rename in progress for this vault path.
    pub(crate) rename_target: Option<String>,
    pub(crate) rename_input: String,
    /// New-folder inline editor visible.
    pub(crate) mkdir_open: bool,
    pub(crate) mkdir_input: String,
    /// Live name filter for the current folder.
    pub(crate) filter: String,
    pub(crate) sort: Sort,
    pub(crate) capacity: Option<Capacity>,
    /// Set when the vault was unlocked; drives the browser fade-in.
    pub(crate) unlocked_at: Option<Instant>,
    /// Set when an error status lands; drives the locked card shake.
    pub(crate) error_pulse: Option<Instant>,
    /// One-shot keyboard-focus request consumed by the browser view.
    pub(crate) focus_request: Option<FocusTarget>,
    pub(crate) show_settings: bool,
    pub(crate) create_mode: bool,
    pub(crate) create_name: String,
    pub(crate) create_size: String,
    pub(crate) auto_lock_minutes: u32,
    pub(crate) auto_lock_enabled: bool,
    pub(crate) anti_capture_enabled: bool,
    anti_capture_applied: Option<bool>,
    pub(crate) recent: RecentStore,
    pub(crate) auto_lock: AutoLock,
    pub(crate) dialog: Option<ActiveDialog>,
    #[cfg(feature = "mount")]
    pub(crate) mount_point_input: String,
    #[cfg(feature = "mount")]
    pub(crate) create_symlinks: bool,
    session: Option<Session>,
    #[cfg(feature = "mount")]
    pub(crate) mounted: Option<crate::mount::MountedVault>,
    cache: Option<CacheSession>,
    pub(crate) notifications: Vec<Notification>,
    pub(crate) notifications_open: bool,
    pub(crate) notifications_read: usize,
    job: Option<job::ActiveJob>,
    pub(crate) flyout: super::flyout::FlyoutState,
    /// Next add lands here instead of [`Self::current_dir`] (the tray panel).
    add_destination: Option<String>,
    tray: Option<super::tray::Tray>,
    tray_attempted: bool,
    /// Window was hidden after close/minimize. The process and any mount stay up.
    hidden_to_tray: bool,
    /// Ignore a stale minimized flag for one restore, so the window is not hidden again.
    suppress_minimize: bool,
    /// Tray menu asked to leave. The next close is allowed through.
    exit_requested: bool,
}

/// Kind of a status notification, derived from its status prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotificationKind {
    Info,
    Success,
    Error,
    Warning,
}

/// One entry in the notification history (bell panel) and the toast stream.
#[derive(Debug, Clone)]
pub(crate) struct Notification {
    pub(crate) text: String,
    pub(crate) kind: NotificationKind,
    pub(crate) at: std::time::Instant,
}

/// How long a toast stays on screen before fading out.
pub(crate) const TOAST_TTL: Duration = Duration::from_secs(5);
/// Last slice of the TTL during which the toast fades out.
pub(crate) const TOAST_FADE: Duration = Duration::from_millis(800);
const MAX_NOTIFICATIONS: usize = 50;

impl Default for App {
    fn default() -> Self {
        Self {
            theme: Theme::Cave,
            palette: Palette::CAVE,
            locale: Locale::En,
            settings: GuiSettings::default(),
            path_input: String::new(),
            password: String::new(),
            no_password: false,
            status: String::new(),
            entries: Vec::new(),
            browser: BrowserTree::default(),
            current_dir: String::new(),
            selected_paths: BTreeSet::new(),
            selection_anchor: None,
            rename_target: None,
            rename_input: String::new(),
            mkdir_open: false,
            mkdir_input: String::new(),
            filter: String::new(),
            sort: Sort::default(),
            capacity: None,
            unlocked_at: None,
            error_pulse: None,
            focus_request: None,
            show_settings: false,
            create_mode: false,
            create_name: "my-sesame".into(),
            create_size: "500MB".into(),
            auto_lock_minutes: 10,
            auto_lock_enabled: true,
            anti_capture_enabled: true,
            anti_capture_applied: None,
            recent: RecentStore::load(),
            auto_lock: AutoLock::new(10),
            dialog: None,
            #[cfg(feature = "mount")]
            mount_point_input: default_mount_point(),
            #[cfg(feature = "mount")]
            create_symlinks: false,
            session: None,
            #[cfg(feature = "mount")]
            mounted: None,
            cache: None,
            notifications: Vec::new(),
            notifications_open: false,
            notifications_read: 0,
            job: None,
            flyout: super::flyout::FlyoutState::default(),
            add_destination: None,
            tray: None,
            tray_attempted: false,
            hidden_to_tray: false,
            suppress_minimize: false,
            exit_requested: false,
        }
    }
}

#[cfg(feature = "mount")]
fn default_mount_point() -> String {
    #[cfg(target_os = "windows")]
    {
        "Z:".into()
    }
    #[cfg(target_os = "macos")]
    {
        "/Volumes/closed-sesame".into()
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        std::env::temp_dir()
            .join("closed-sesame-mount")
            .to_string_lossy()
            .into_owned()
    }
}

/// Hold the FUSE mount's writer mutex while the GUI mutates the container
/// out-of-band, so a FUSE flush cannot interleave mid-mutation. On backends
/// without shared in-process state this is a no-op.
#[cfg(feature = "mount")]
fn mount_guard(
    mounted: &Option<crate::mount::MountedVault>,
) -> Option<std::sync::MutexGuard<'_, crate::container::ContainerWriter>> {
    mounted.as_ref().and_then(|mounted| mounted.mutation_lock())
}

impl App {
    pub(crate) fn new(settings: GuiSettings) -> Self {
        if let Some(locale) = settings.locale() {
            crate::i18n::init(Some(locale));
        }
        let mut app = Self::default();
        app.settings = settings.clone();
        app.anti_capture_enabled = settings.anti_capture_enabled;
        app.theme = settings.theme;
        app.palette = settings.theme.palette();
        app.auto_lock_enabled = settings.auto_lock_enabled;
        app.auto_lock_minutes = settings.auto_lock_minutes.clamp(1, 60);
        app.auto_lock = AutoLock::new(app.auto_lock_minutes);
        app.auto_lock.set_enabled(app.auto_lock_enabled);
        if let Some(locale) = app.settings.locale() {
            app.locale = locale;
        }
        #[cfg(debug_assertions)]
        app.seed_demo();
        app
    }

    /// Screenshot/demo seeding, debug builds only. `CLOSED_SESAME_DEMO` may be
    /// `dark`, `light`, `dark-locked`, or `light-locked`; without `-locked`
    /// a throwaway vault with sample entries is created and unlocked.
    #[cfg(debug_assertions)]
    fn seed_demo(&mut self) {
        let Ok(spec) = std::env::var("CLOSED_SESAME_DEMO") else {
            return;
        };
        self.theme = if spec.contains("light") {
            Theme::Parchment
        } else {
            Theme::Cave
        };
        self.palette = self.theme.palette();
        self.locale = Locale::En;
        self.settings.locale = Some(super::settings::LocaleCode::En);
        crate::i18n::init(Some(Locale::En));
        // Demo mode exists to take screenshots; content protection would
        // blank the window in every capture tool, so turn it off here only.
        self.anti_capture_enabled = false;
        // In-memory only: lets screenshots show the recent list without
        // touching the user's real recent-vaults.json.
        self.recent.entries = [
            "/vaults/personal.sesame",
            "/vaults/work-secrets.sesame",
            "/vaults/backup-2026.sesame",
            "/vaults/archive.sesame",
            "/vaults/family-photos.sesame",
        ]
        .into_iter()
        .map(|path| super::recent::RecentVault {
            path: PathBuf::from(path),
            last_opened: 1_785_520_000,
        })
        .collect();
        if spec.contains("locked") {
            if spec.contains("create") {
                self.create_mode = true;
                self.create_name = "family-photos".into();
                self.password = "correct horse battery".into();
            }
            if spec.contains("error") {
                self.set_status(ts!(
                    "gui.status.error",
                    error = "wrong password or corrupted header"
                ));
            }
            return;
        }
        let dir = std::env::temp_dir().join(format!("closed-sesame-demo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let samples = [
            ("docs", "Q3-Report.pdf", 1_200_000usize),
            ("docs", "roadmap.md", 18_400),
            ("docs/specs", "format-v3.txt", 42_100),
            ("photos", "kyoto-sunrise.raw", 812_000),
            ("", "notes.txt", 3_412),
            ("", "recovery-key.txt", 812),
            ("", "archive-2026.zip", 402_000),
        ];
        for (subdir, name, size) in samples {
            let parent = if subdir.is_empty() {
                dir.clone()
            } else {
                dir.join(subdir)
            };
            if std::fs::create_dir_all(&parent).is_err() {
                return;
            }
            if std::fs::write(parent.join(name), vec![b'#'; size]).is_err() {
                return;
            }
        }
        let Ok(opened) = actions::create(&dir.join("demo-vault"), 8 * 1024 * 1024, "", true) else {
            return;
        };
        let sources = [
            "docs",
            "photos",
            "notes.txt",
            "recovery-key.txt",
            "archive-2026.zip",
        ]
        .into_iter()
        .map(|name| dir.join(name))
        .collect::<Vec<_>>();
        let result = actions::add_many(&opened.session, sources, "");
        self.path_input = opened.session.path().to_string_lossy().into_owned();
        self.session = Some(opened.session);
        self.install_entries(result.entries);
        self.cache = CacheSession::start().ok();
        self.unlocked_at = Some(Instant::now());
        self.set_status(ts!("gui.status.demo_seeded"));
        // Optional sub-states so every screen can be captured without input.
        if spec.contains("settings") {
            self.show_settings = true;
        }
        if spec.contains("select") {
            self.selected_paths.insert("notes.txt".into());
            self.selected_paths.insert("archive-2026.zip".into());
            self.selection_anchor = Some("notes.txt".into());
        }
        if spec.contains("delete") {
            self.dialog = Some(ActiveDialog::DeleteConfirm {
                paths: vec!["notes.txt".into(), "archive-2026.zip".into()],
            });
        }
        if spec.contains("search") {
            self.filter = "re".into();
        }
        if spec.contains("rename") {
            self.begin_rename("notes.txt".into());
        }
        if spec.contains("mkdir") {
            self.mkdir_open = true;
            self.mkdir_input = "invoices".into();
        }
        if spec.contains("bell") {
            self.notifications_open = true;
        }
    }

    pub(crate) fn needs_locale_setup(&self) -> bool {
        self.settings.locale.is_none()
    }

    pub(crate) fn is_unlocked(&self) -> bool {
        self.session.is_some()
    }

    pub(crate) fn has_job(&self) -> bool {
        self.job.is_some()
    }

    pub(crate) fn current_job(&self) -> Option<&job::ActiveJob> {
        self.job.as_ref()
    }

    pub(crate) fn touch(&mut self) {
        self.auto_lock.touch();
    }

    pub(crate) fn set_status(&mut self, status: impl Into<String>) {
        self.status.zeroize();
        let text = status.into();
        let kind = if text.starts_with('✓') {
            NotificationKind::Success
        } else if text.starts_with('✗') {
            NotificationKind::Error
        } else if text.starts_with('⚠') {
            NotificationKind::Warning
        } else {
            NotificationKind::Info
        };
        if kind == NotificationKind::Error {
            self.error_pulse = Some(Instant::now());
        }
        self.notifications.push(Notification {
            text: text.clone(),
            kind,
            at: std::time::Instant::now(),
        });
        if self.notifications.len() > MAX_NOTIFICATIONS {
            let overflow = self.notifications.len() - MAX_NOTIFICATIONS;
            self.notifications.drain(..overflow);
            self.notifications_read = self.notifications_read.saturating_sub(overflow);
        }
        self.status = text;
    }

    /// Unread notifications since the bell panel was last opened.
    pub(crate) fn unread_count(&self) -> usize {
        self.notifications.len() - self.notifications_read.min(self.notifications.len())
    }

    pub(crate) fn toggle_notifications(&mut self) {
        self.notifications_open = !self.notifications_open;
        if self.notifications_open {
            self.notifications_read = self.notifications.len();
        }
    }

    pub(crate) fn clear_notifications(&mut self) {
        self.notifications.clear();
        self.notifications_read = 0;
    }

    /// Expire the current toast immediately (history entry is kept).
    pub(crate) fn dismiss_toast(&mut self) {
        if let Some(latest) = self.notifications.last_mut() {
            latest.at = std::time::Instant::now() - TOAST_TTL;
        }
    }

    fn install_entries(&mut self, entries: Vec<FileEntryInfo>) {
        self.browser = BrowserTree::from_entries(
            &entries
                .iter()
                .map(|entry| crate::FileEntry {
                    path: entry.path.clone(),
                    original_path: None,
                    size: entry.size,
                    start_page: 0,
                    page_count: 0,
                    crc32: 0,
                    created_at: 0,
                    modified_at: entry.modified_at,
                    has_symlink: false,
                    is_directory: entry.is_dir,
                })
                .collect::<Vec<_>>(),
        );
        self.entries = entries;
        // The folder we were browsing may have been deleted or renamed
        // (possibly through a mount); fall back to the nearest ancestor so the
        // breadcrumb never points at a path that no longer exists.
        if !self.browser.has_dir(&self.current_dir) {
            self.current_dir = self.browser.nearest_existing_dir(&self.current_dir);
            self.clear_selection();
        }
        if !self.browser.has_dir(&self.flyout.dir) {
            self.flyout.dir = self.browser.nearest_existing_dir(&self.flyout.dir);
            self.flyout.selected = None;
        }
        // Drop selections that vanished.
        let live: BTreeSet<&str> = self
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect();
        self.selected_paths
            .retain(|path| live.contains(path.as_str()));
        if self
            .flyout
            .selected
            .as_ref()
            .is_some_and(|path| !live.contains(path.as_str()))
        {
            self.flyout.selected = None;
        }
        if self
            .selection_anchor
            .as_ref()
            .is_some_and(|anchor| !live.contains(anchor.as_str()))
        {
            self.selection_anchor = self.selected_paths.iter().next().cloned();
        }
        if self
            .rename_target
            .as_ref()
            .is_some_and(|target| !live.contains(target.as_str()))
        {
            self.cancel_rename();
        }
        self.capacity = self
            .session
            .as_ref()
            .and_then(|session| actions::capacity(session).ok());
    }

    fn install_opened(&mut self, opened: actions::OpenResult, message: String) {
        self.password.zeroize();
        self.password.clear();
        self.path_input = opened.session.path().to_string_lossy().into_owned();
        self.session = Some(opened.session);
        self.current_dir.clear();
        self.clear_selection();
        self.filter.clear();
        self.install_entries(opened.entries);
        self.cache = CacheSession::start().ok();
        self.recent.record(&self.path_input);
        self.auto_lock.touch();
        self.unlocked_at = Some(Instant::now());
        self.set_status(message);
    }

    pub(crate) fn begin_rename(&mut self, path: String) {
        self.rename_input = path.rsplit('/').next().unwrap_or(&path).to_owned();
        self.rename_target = Some(path);
        self.focus_request = Some(FocusTarget::Rename);
    }

    pub(crate) fn cancel_rename(&mut self) {
        self.rename_target = None;
        self.rename_input.clear();
    }

    fn commit_rename(&mut self) {
        let Some(old) = self.rename_target.clone() else {
            return;
        };
        let new_name = self.rename_input.trim().to_owned();
        if new_name.is_empty() {
            self.set_status(ts!("gui.status.new_name_required"));
            return;
        }
        let new_path = match old.rsplit_once('/') {
            Some((parent, _)) => format!("{parent}/{new_name}"),
            None => new_name.clone(),
        };
        if new_path == old {
            self.cancel_rename();
            return;
        }
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let result = {
            #[cfg(feature = "mount")]
            let _guard = mount_guard(&self.mounted);
            actions::rename(session, &old, &new_path)
        };
        match result {
            Ok(()) => {
                self.cancel_rename();
                self.selected_paths.clear();
                self.selected_paths.insert(new_path.clone());
                self.selection_anchor = Some(new_path);
                self.refresh();
            }
            Err(error) => self.set_status(ts!("gui.status.error", error = error.localized())),
        }
    }

    fn open(&mut self) {
        let path = PathBuf::from(self.path_input.trim());
        match actions::open(&path, &self.password, self.no_password) {
            Ok(opened) => self.install_opened(
                opened,
                ts!("gui.status.opened", path = path.display().to_string()),
            ),
            Err(error) => self.set_status(ts!("gui.status.error", error = error.localized())),
        }
    }

    fn choose_create_path(&mut self) {
        let name = self.create_name.trim();
        if name.is_empty() {
            self.set_status(ts!("gui.status.name_required"));
            return;
        }
        let data_size = match parse_size(&self.create_size) {
            Ok(size) => size,
            Err(error) => {
                self.set_status(ts!("gui.status.error", error = error.localized()));
                return;
            }
        };
        // Validate credentials before the native save dialog so the user is
        // not bounced through a file picker only to hit "password empty".
        if !self.no_password && self.password.is_empty() {
            self.set_status(ts!(
                "gui.status.error",
                error = crate::SesameError::InvalidInput(crate::i18n::keys::PASSWORD_EMPTY.into())
                    .localized()
            ));
            return;
        }
        let file_name = if name.ends_with(".sesame") {
            name.to_owned()
        } else {
            format!("{name}.sesame")
        };
        let Some(path) = self.with_native_modal(|| {
            rfd::FileDialog::new()
                .add_filter(&ts!("gui.sesame_filter"), &["sesame"])
                .set_file_name(&file_name)
                .save_file()
        }) else {
            return;
        };
        match actions::create(&path, data_size, &self.password, self.no_password) {
            Ok(opened) => self.install_opened(opened, ts!("gui.status.sesame_created")),
            Err(error) => self.set_status(ts!("gui.status.error", error = error.localized())),
        }
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        let Some(session) = self.session.as_mut() else {
            self.set_status(ts!("gui.status.sesame_locked"));
            return;
        };
        #[cfg(feature = "mount")]
        {
            session.create_symlinks = self.create_symlinks && self.mounted.is_some();
        }
        let current_dir = self
            .add_destination
            .take()
            .unwrap_or_else(|| self.current_dir.clone());
        let container = session.path().to_path_buf();
        let credentials = session.credentials().clone();
        let create_symlinks = session.create_symlinks;
        #[cfg(feature = "mount")]
        let ticket = self.mutation_ticket();
        let total = paths.len() as u64;
        self.job = Some(job::ActiveJob::spawn(
            ts!("gui.job.add"),
            total,
            move |progress| {
                #[cfg(feature = "mount")]
                let _guard = ticket.as_ref().map(|ticket| ticket.lock());
                let worker = actions::Session::for_worker(container, credentials, create_symlinks);
                let result = actions::add_many_with_progress(
                    &worker,
                    paths,
                    &current_dir,
                    &|done, total, detail| progress.report(done, total, detail),
                );
                Ok(JobOutput::Add(result))
            },
        ));
        self.touch();
    }

    fn finish_add(&mut self, result: actions::BatchAddResult) {
        let added_count = result.added_count();
        let rejected_count = result.rejected_count();
        let refresh_error = result.refresh_error.clone();
        let outcomes = result.outcomes.clone();
        if refresh_error.is_none() {
            self.install_entries(result.entries);
        }
        #[cfg(feature = "mount")]
        if added_count > 0 {
            self.sync_mounted_vfs();
        }
        #[cfg(feature = "mount")]
        let mut symlink_errors = Vec::new();
        #[cfg(feature = "mount")]
        if let Some(session) = self.session.as_mut() {
            if session.create_symlinks {
                if let Some(mounted) = self.mounted.as_ref() {
                    let mount_point = mounted.mount_point().to_path_buf();
                    for outcome in &outcomes {
                        if let actions::AddOutcome::Added { links } = outcome {
                            for (path, name) in links {
                                let result = if path.is_dir() {
                                    session
                                        .symlinks
                                        .move_dir_with_symlink(path, &mount_point, name)
                                } else {
                                    session.symlinks.move_with_symlink(path, &mount_point, name)
                                };
                                if let Err(error) = result {
                                    symlink_errors.push((path.clone(), error.localized()));
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut status = ts!(
            "gui.status.added_summary",
            added = added_count,
            rejected = rejected_count
        );
        if let Some(error) = refresh_error {
            status.push_str(&ts!("gui.status.refresh_failed", error = error));
        }
        #[cfg(feature = "mount")]
        for (path, error) in symlink_errors {
            status.push_str(&format!(
                "\n{}",
                ts!(
                    "gui.status.symlink_failed",
                    path = path.display().to_string(),
                    error = error
                )
            ));
        }
        for outcome in outcomes {
            if let actions::AddOutcome::Rejected { path, reason } = outcome {
                status.push_str(&format!(
                    "\n{}",
                    ts!(
                        "gui.status.rejected_item",
                        path = path.display().to_string(),
                        reason = localized_message(&reason)
                    )
                ));
            }
        }
        self.set_status(status);
        self.touch();
    }

    fn refresh(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        match actions::refresh(session) {
            Ok(entries) => {
                self.install_entries(entries);
                self.set_status(ts!("gui.status.list_refreshed"));
            }
            Err(error) => self.set_status(ts!(
                "gui.status.refresh_failed_short",
                error = error.localized()
            )),
        }
        #[cfg(feature = "mount")]
        self.sync_mounted_vfs();
        self.touch();
    }

    #[cfg(feature = "mount")]
    fn sync_mounted_vfs(&self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let Some(mounted) = self.mounted.as_ref() else {
            return;
        };
        let _ = mounted.refresh_vault(session.path(), session.credentials());
    }

    pub(crate) fn lock(&mut self) {
        let reopen = self.path_input.clone();
        let unsynced = self.flush_cache_on_lock();
        if let Some(cache) = self.cache.take() {
            if unsynced.is_empty() {
                cache.wipe();
            } else {
                cache.wipe_preserving(&unsynced);
            }
        }
        #[cfg(feature = "mount")]
        {
            self.unmount_vault();
            if let Some(session) = self.session.as_mut() {
                let _ = session.symlinks.remove_all();
            }
        }
        self.password.zeroize();
        self.password.clear();
        self.path_input.zeroize();
        self.path_input.clear();
        self.status.zeroize();
        self.status.clear();
        self.entries.clear();
        self.browser = BrowserTree::default();
        self.current_dir.clear();
        self.clear_selection();
        self.cancel_rename();
        self.mkdir_open = false;
        self.mkdir_input.clear();
        self.filter.clear();
        self.capacity = None;
        self.unlocked_at = None;
        self.session = None;
        self.show_settings = false;
        self.dialog = None;
        self.notifications_open = false;
        self.add_destination = None;
        self.flyout.on_locked(reopen);
        if !unsynced.is_empty() {
            self.set_status(ts!(
                "gui.status.cache_unsynced_kept",
                count = unsynced.len()
            ));
        }
    }

    /// Flush every dirty cache file into the vault before locking.
    /// Conflicts are resolved as keep-both (never lose edits). Returns the
    /// cache paths that could not be synced and must be preserved on disk.
    fn flush_cache_on_lock(&mut self) -> Vec<PathBuf> {
        let dirty: Vec<String> = match self.cache.as_mut() {
            Some(cache) => {
                cache.poll_events();
                cache.mark_all_dirty();
                cache
                    .entries()
                    .iter()
                    .filter(|entry| entry.is_dirty())
                    .map(|entry| entry.container_path.clone())
                    .collect()
            }
            None => Vec::new(),
        };
        let mut failed = Vec::new();
        for container_path in dirty {
            let snapshot = self
                .cache
                .as_ref()
                .and_then(|cache| cache.entry(&container_path))
                .cloned();
            let Some(entry) = snapshot else {
                continue;
            };
            let Ok(content) = std::fs::read(&entry.cache_path) else {
                continue;
            };
            let current_modified = self.session.as_ref().and_then(|session| {
                actions::vault_modified_at(session, &container_path)
                    .ok()
                    .flatten()
            });
            let ok = match cache::plan_writeback(&entry, &content, current_modified) {
                WritebackPlan::Unchanged => true,
                WritebackPlan::Overwrite => self.write_back_overwrite(&container_path, &content),
                WritebackPlan::Conflict | WritebackPlan::MissingFromVault => {
                    self.write_back_keep_both(&container_path, &content)
                }
            };
            if !ok {
                failed.push(entry.cache_path.clone());
            }
        }
        failed
    }

    /// Read the current plaintext of a tracked cache file.
    fn read_cache_content(&self, container_path: &str) -> Option<Vec<u8>> {
        let entry = self.cache.as_ref()?.entry(container_path)?;
        std::fs::read(&entry.cache_path).ok()
    }

    /// Overwrite the vault entry with cache content (two-phase commit).
    fn write_back_overwrite(&mut self, container_path: &str, content: &[u8]) -> bool {
        let Some(session) = self.session.as_ref() else {
            return false;
        };
        let result = {
            #[cfg(feature = "mount")]
            let _guard = mount_guard(&self.mounted);
            actions::replace_vault_file(session, container_path, content)
        };
        match result {
            Ok(entry) => {
                let modified_at = entry.modified_at;
                let content_crc32 = crc32(content);
                if let Some(cache) = self.cache.as_mut() {
                    if let Some(cache_entry) = cache.entry_mut(container_path) {
                        cache_entry.mark_synced(modified_at, content_crc32);
                    }
                }
                self.refresh();
                self.set_status(ts!("gui.status.cache_synced", path = container_path));
                true
            }
            Err(error) => {
                self.set_status(ts!(
                    "gui.status.cache_sync_failed",
                    path = container_path,
                    error = error.localized()
                ));
                false
            }
        }
    }

    /// Save cache content as a new timestamped conflict entry, keeping the
    /// vault original untouched. The cache file then tracks the new entry.
    fn write_back_keep_both(&mut self, container_path: &str, content: &[u8]) -> bool {
        let conflict_path = self.unique_conflict_path(container_path);
        let Some(session) = self.session.as_ref() else {
            return false;
        };
        let result = {
            #[cfg(feature = "mount")]
            let _guard = mount_guard(&self.mounted);
            actions::add_vault_file(session, &conflict_path, content)
        };
        match result {
            Ok(entry) => {
                let modified_at = entry.modified_at;
                let content_crc32 = crc32(content);
                if let Some(cache) = self.cache.as_mut() {
                    if let Some(cache_entry) = cache.entry_mut(container_path) {
                        cache_entry.container_path = conflict_path.clone();
                        cache_entry.mark_synced(modified_at, content_crc32);
                    }
                }
                self.refresh();
                self.set_status(ts!("gui.status.cache_kept_both", path = conflict_path));
                true
            }
            Err(error) => {
                self.set_status(ts!(
                    "gui.status.cache_sync_failed",
                    path = container_path,
                    error = error.localized()
                ));
                false
            }
        }
    }

    fn unique_conflict_path(&self, container_path: &str) -> String {
        let secs = cache::now_secs();
        for attempt in 0..100u64 {
            let candidate = cache::conflict_copy_path(container_path, secs + attempt);
            if !self.entries.iter().any(|entry| entry.path == candidate) {
                return candidate;
            }
        }
        cache::conflict_copy_path(container_path, secs)
    }

    fn discard_cache_entry(&mut self, container_path: &str) {
        let cache_path = self
            .cache
            .as_ref()
            .and_then(|cache| cache.entry(container_path))
            .map(|entry| entry.cache_path.clone());
        if let Some(cache) = self.cache.as_mut() {
            cache.remove(container_path);
        }
        if let Some(path) = cache_path {
            cache::zero_and_remove(&path);
        }
        self.set_status(ts!("gui.status.cache_discarded", path = container_path));
    }

    /// Open a vault file in the writable session cache and the external app.
    fn open_in_cache(&mut self, container_path: &str) {
        // Mounted + a real local mount point: open straight through the
        // mount so edits flow via the mount write path, not the cache.
        #[cfg(feature = "mount")]
        if let Some(mounted) = self.mounted.as_ref() {
            let candidate =
                crate::app::symlink::join_mount_path(mounted.mount_point(), container_path);
            if candidate.is_file() {
                match super::open_external::open_path(&candidate) {
                    Ok(()) => self.set_status(ts!(
                        "gui.status.opened_file",
                        path = candidate.display().to_string()
                    )),
                    Err(error) => {
                        self.set_status(ts!("gui.status.error", error = error.localized()))
                    }
                }
                self.touch();
                return;
            }
        }

        if self.cache.is_none() {
            self.cache = CacheSession::start().ok();
        }
        // Already tracked: flush pending edits first, then rebuild the cache
        // file from the vault instead of reusing a stale copy.
        let has_existing = self
            .cache
            .as_ref()
            .is_some_and(|cache| cache.entry(container_path).is_some());
        if has_existing {
            self.sync_cache_entry(container_path);
            if self.dialog.is_some() {
                // A conflict dialog is pending; let the user resolve it first.
                return;
            }
            let old_path = self
                .cache
                .as_ref()
                .and_then(|cache| cache.entry(container_path))
                .map(|entry| entry.cache_path.clone());
            if let Some(cache) = self.cache.as_mut() {
                cache.remove(container_path);
            }
            if let Some(path) = old_path {
                cache::zero_and_remove(&path);
            }
        }
        let Some(session) = self.session.as_ref() else {
            self.set_status(ts!("gui.status.sesame_locked"));
            return;
        };
        let (data, vault_entry) = match actions::read_vault_file(session, container_path) {
            Ok(read) => read,
            Err(error) => {
                self.set_status(ts!("gui.status.error", error = error.localized()));
                return;
            }
        };
        let Some(cache) = self.cache.as_mut() else {
            self.set_status(ts!("gui.status.cache_unavailable"));
            return;
        };
        let cache_path = cache.fresh_cache_path(container_path);
        if let Err(error) = std::fs::write(&cache_path, &data) {
            self.set_status(ts!("gui.status.error", error = error.to_string()));
            return;
        }
        cache.register(cache::CacheEntry::new(
            container_path.to_owned(),
            cache_path.clone(),
            vault_entry.modified_at,
            crc32(&data),
        ));
        match super::open_external::open_path(&cache_path) {
            Ok(()) => self.set_status(ts!(
                "gui.status.opened_file",
                path = cache_path.display().to_string()
            )),
            Err(error) => self.set_status(ts!("gui.status.error", error = error.localized())),
        }
        self.touch();
    }

    /// Poll the cache watcher and write back any settled edits.
    fn poll_cache(&mut self, ctx: &egui::Context) {
        let has_dirty = match self.cache.as_mut() {
            Some(cache) => {
                cache.poll_events();
                cache.entries().iter().any(|entry| entry.is_dirty())
            }
            None => return,
        };
        if has_dirty {
            ctx.request_repaint_after(cache::SETTLE_WINDOW + Duration::from_millis(200));
        }
        if self.dialog.is_some() {
            return;
        }
        let settled = match self.cache.as_mut() {
            Some(cache) => cache.take_settled(),
            None => return,
        };
        for container_path in settled {
            self.sync_cache_entry(&container_path);
        }
    }

    fn sync_cache_entry(&mut self, container_path: &str) {
        let snapshot = self
            .cache
            .as_ref()
            .and_then(|cache| cache.entry(container_path))
            .cloned();
        let Some(entry) = snapshot else {
            return;
        };
        let content = match std::fs::read(&entry.cache_path) {
            Ok(content) => content,
            Err(_) => {
                // Cache file was removed externally; stop tracking it.
                if let Some(cache) = self.cache.as_mut() {
                    cache.remove(container_path);
                }
                return;
            }
        };
        let current_modified = self.session.as_ref().and_then(|session| {
            actions::vault_modified_at(session, container_path)
                .ok()
                .flatten()
        });
        match cache::plan_writeback(&entry, &content, current_modified) {
            WritebackPlan::Unchanged => {}
            WritebackPlan::Overwrite => {
                self.write_back_overwrite(container_path, &content);
            }
            WritebackPlan::Conflict | WritebackPlan::MissingFromVault => {
                if let Some(cache) = self.cache.as_mut() {
                    if let Some(cache_entry) = cache.entry_mut(container_path) {
                        cache_entry.awaiting_decision = true;
                    }
                }
                self.dialog = Some(ActiveDialog::WritebackConflict {
                    container_path: container_path.to_owned(),
                });
            }
        }
    }

    /// Mounting can take seconds (WinFsp start plus an external accessibility
    /// probe), so it runs as a background job instead of freezing the window.
    #[cfg(feature = "mount")]
    fn mount_vault(&mut self) {
        if self.job.is_some() || self.mounted.is_some() {
            return;
        }
        let Some(session) = self.session.as_ref() else {
            self.set_status(ts!("gui.status.sesame_locked"));
            return;
        };
        let mount_point = PathBuf::from(self.mount_point_input.trim());
        let container = session.path().to_path_buf();
        let credentials = session.credentials().clone();
        self.job = Some(job::ActiveJob::spawn(ts!("gui.job.mount"), 0, move |_| {
            let mounted = actions::mount(&credentials, &container, &mount_point)?;
            Ok(JobOutput::Mounted { mounted, container })
        }));
        self.touch();
    }

    #[cfg(feature = "mount")]
    fn finish_mount(&mut self, mounted: crate::mount::MountedVault, container: PathBuf) {
        // The vault may have been locked (tray, auto-lock) while mounting;
        // dropping `mounted` unmounts it again.
        if self.session.as_ref().map(|session| session.path()) != Some(container.as_path()) {
            return;
        }
        let mount_point_path = mounted.mount_point().to_path_buf();
        let point = mount_point_path.display().to_string();
        #[cfg(target_os = "macos")]
        if point.starts_with("http://") || point.starts_with("https://") {
            crate::mount::finder_mount_volume(&point);
        }
        let mut restore_errors = Vec::new();
        if let Some(session) = self.session.as_mut() {
            // Skip the full metadata re-read when there are no
            // symlinks to restore (the common case).
            if !session.symlinks.is_empty() {
                if let Ok(entries) =
                    vault::list_with_credentials(session.path(), session.credentials())
                {
                    restore_errors = session
                        .symlinks
                        .restore_symlinks(&entries, &mount_point_path);
                }
            }
        }
        self.mounted = Some(mounted);
        let mut status = ts!("gui.status.mounted", point = point);
        for (path, error) in restore_errors {
            status.push_str(&format!(
                "\n{}",
                ts!(
                    "gui.status.symlink_failed",
                    path = path.display().to_string(),
                    error = error
                )
            ));
        }
        self.set_status(status);
    }

    #[cfg(feature = "mount")]
    fn unmount_vault(&mut self) {
        if let Some(session) = self.session.as_mut() {
            let _ = session.symlinks.remove_all();
        }
        if let Some(mut mounted) = self.mounted.take() {
            if let Err(error) = mounted.unmount() {
                self.set_status(ts!("gui.status.unmount_failed", error = error.localized()));
                self.mounted = Some(mounted);
            }
        }
    }

    pub(crate) fn request_add_files(&mut self, paths: Vec<PathBuf>) {
        #[cfg(feature = "mount")]
        {
            if self.create_symlinks
                && self.mounted.is_some()
                && !self.settings.hints.skip_symlink_info
            {
                self.dialog = Some(ActiveDialog::SymlinkInfo {
                    paths,
                    dont_show_again: false,
                });
                return;
            }
        }
        self.add_files(paths);
    }

    fn clear_selection(&mut self) {
        self.selected_paths.clear();
        self.selection_anchor = None;
    }

    fn apply_selection(&mut self, path: String, ctrl: bool, shift: bool, ordered_paths: &[String]) {
        if shift {
            let anchor = self
                .selection_anchor
                .clone()
                .or_else(|| self.selected_paths.iter().next().cloned())
                .unwrap_or_else(|| path.clone());
            if let (Some(start), Some(end)) = (
                ordered_paths.iter().position(|item| item == &anchor),
                ordered_paths.iter().position(|item| item == &path),
            ) {
                let (from, to) = if start <= end {
                    (start, end)
                } else {
                    (end, start)
                };
                if !ctrl {
                    self.selected_paths.clear();
                }
                for item in &ordered_paths[from..=to] {
                    self.selected_paths.insert(item.clone());
                }
            } else if !ctrl {
                self.selected_paths.clear();
                self.selected_paths.insert(path.clone());
            }
            self.selection_anchor = Some(anchor);
        } else if ctrl {
            if !self.selected_paths.remove(&path) {
                self.selected_paths.insert(path.clone());
            }
            self.selection_anchor = Some(path);
        } else {
            self.selected_paths.clear();
            self.selected_paths.insert(path);
            self.selection_anchor = self.selected_paths.iter().next().cloned();
        }
    }

    fn begin_extract(&mut self, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        if self.settings.hints.skip_extract_info {
            self.perform_extract(paths);
        } else {
            self.dialog = Some(ActiveDialog::ExtractInfo {
                paths,
                dont_show_again: false,
            });
        }
    }

    fn perform_extract(&mut self, paths: Vec<String>) {
        self.start_extract(paths, false);
    }

    fn start_extract(&mut self, paths: Vec<String>, move_out: bool) {
        if paths.is_empty() || self.job.is_some() {
            return;
        }
        let Some(output) = self.with_native_modal(|| rfd::FileDialog::new().pick_folder()) else {
            return;
        };
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let container = session.path().to_path_buf();
        let credentials = session.credentials().clone();
        #[cfg(feature = "mount")]
        let mount_point = self
            .mounted
            .as_ref()
            .map(|mounted| mounted.mount_point().to_path_buf());
        #[cfg(feature = "mount")]
        let ticket = self.mutation_ticket();
        let total = paths.len() as u64;
        let title = if move_out {
            ts!("gui.job.move_out")
        } else {
            ts!("gui.job.extract")
        };
        self.job = Some(job::ActiveJob::spawn(title, total, move |progress| {
            #[cfg(feature = "mount")]
            let _guard = ticket.as_ref().map(|ticket| ticket.lock());
            #[cfg(feature = "mount")]
            let options = crate::app::vault::ExtractOptions {
                mount_point: mount_point.as_deref(),
            };
            #[cfg(not(feature = "mount"))]
            let options = crate::app::vault::ExtractOptions::default();
            let names: Vec<&str> = paths.iter().map(String::as_str).collect();
            let outcome = crate::app::vault::extract_many_reporting(
                &container,
                &credentials,
                &names,
                &output,
                options,
                &|done, total| progress.report(done, total, ""),
            )?;
            let mut removed = Vec::new();
            if move_out {
                for path in &paths {
                    removed.extend(crate::app::vault::remove_path_with_credentials(
                        &container,
                        &credentials,
                        path,
                    )?);
                }
            }
            let _ = outcome;
            Ok(JobOutput::Extracted {
                count: names.len(),
                removed,
            })
        }));
    }

    fn perform_delete(&mut self, paths: Vec<String>) {
        if paths.is_empty() || self.job.is_some() {
            return;
        }
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let container = session.path().to_path_buf();
        let credentials = session.credentials().clone();
        #[cfg(feature = "mount")]
        let ticket = self.mutation_ticket();
        let total = paths.len() as u64;
        self.job = Some(job::ActiveJob::spawn(
            ts!("gui.job.delete"),
            total,
            move |progress| {
                #[cfg(feature = "mount")]
                let _guard = ticket.as_ref().map(|ticket| ticket.lock());
                let mut removed = Vec::new();
                for (index, path) in paths.iter().enumerate() {
                    progress.report(index as u64 + 1, total, path.clone());
                    removed.extend(crate::app::vault::remove_path_with_credentials(
                        &container,
                        &credentials,
                        path,
                    )?);
                }
                Ok(JobOutput::Deleted(removed))
            },
        ));
    }

    #[cfg(feature = "mount")]
    fn mutation_ticket(&self) -> Option<crate::mount::MutationTicket> {
        self.mounted
            .as_ref()
            .and_then(|mounted| mounted.mutation_ticket())
    }

    fn ensure_unmounted(&mut self) -> bool {
        #[cfg(feature = "mount")]
        if self.mounted.is_some() {
            self.set_status(ts!("gui.status.unmount_first"));
            return false;
        }
        true
    }

    fn begin_transfer(&mut self, copy: bool) {
        let paths: Vec<String> = self.selected_paths.iter().cloned().collect();
        if paths.is_empty() {
            return;
        }
        self.dialog = Some(ActiveDialog::Transfer {
            copy,
            paths,
            destination: self.current_dir.clone(),
        });
    }

    fn start_transfer(&mut self, copy: bool, paths: Vec<String>, destination: String) {
        if self.job.is_some() {
            return;
        }
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let pairs = match actions::plan_transfers(&self.entries, &paths, &destination, copy) {
            Ok(pairs) => pairs,
            Err(error) => {
                self.set_status(ts!("gui.status.error", error = error.localized()));
                return;
            }
        };
        if pairs.is_empty() {
            self.set_status(ts!("gui.status.nothing_to_do"));
            return;
        }
        let container = session.path().to_path_buf();
        let credentials = session.credentials().clone();
        #[cfg(feature = "mount")]
        let ticket = self.mutation_ticket();
        let total = pairs.len() as u64;
        let title = if copy {
            ts!("gui.job.copy")
        } else {
            ts!("gui.job.move")
        };
        self.job = Some(job::ActiveJob::spawn(title, total, move |progress| {
            #[cfg(feature = "mount")]
            let _guard = ticket.as_ref().map(|ticket| ticket.lock());
            if copy {
                crate::app::vault::copy_many(&container, &credentials, &pairs, &|done, total| {
                    progress.report(done, total, "");
                })?;
            } else {
                crate::app::vault::move_many(&container, &credentials, &pairs, &|done, total| {
                    progress.report(done, total, "");
                })?;
            }
            Ok(JobOutput::Transferred {
                count: pairs.len(),
                copied: copy,
            })
        }));
    }

    fn change_password(&mut self, mut password: String, mut confirm: String) {
        if password != confirm {
            password.zeroize();
            confirm.zeroize();
            self.set_status(ts!(
                "gui.status.error",
                error =
                    crate::SesameError::InvalidInput(crate::i18n::keys::PASSWORD_MISMATCH.into())
                        .localized()
            ));
            return;
        }
        let clear = password.is_empty();
        if !self.ensure_unmounted() {
            password.zeroize();
            confirm.zeroize();
            return;
        }
        let Some(session) = self.session.as_ref() else {
            password.zeroize();
            return;
        };
        let result = if clear {
            crate::app::vault::clear_password(session.path(), session.credentials())
        } else {
            crate::app::vault::change_password(session.path(), session.credentials(), &password)
        };
        password.zeroize();
        confirm.zeroize();
        match result {
            Ok(kek) => {
                if let Some(session) = self.session.as_mut() {
                    session.set_credentials(kek);
                }
                let key = if clear {
                    "gui.status.password_cleared"
                } else {
                    "gui.status.password_changed"
                };
                self.set_status(ts!(key));
            }
            Err(error) => self.set_status(ts!("gui.status.error", error = error.localized())),
        }
    }

    fn grow_vault(&mut self, size: String) {
        if !self.ensure_unmounted() {
            return;
        }
        let additional = match parse_size(&size) {
            Ok(value) => value as u64,
            Err(error) => {
                self.set_status(ts!("gui.status.error", error = error.localized()));
                return;
            }
        };
        let Some(session) = self.session.as_ref() else {
            return;
        };
        match crate::app::vault::grow(session.path(), session.credentials(), additional) {
            Ok(capacity) => {
                self.refresh();
                self.set_status(ts!(
                    "gui.status.grew",
                    capacity = crate::app::fmt_size(capacity)
                ));
            }
            Err(error) => self.set_status(ts!("gui.status.error", error = error.localized())),
        }
    }

    fn start_verify(&mut self) {
        if self.job.is_some() {
            return;
        }
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let path = session.path().to_path_buf();
        let credentials = session.credentials().clone();
        self.job = Some(job::ActiveJob::spawn(
            ts!("gui.job.verify"),
            0,
            move |progress| {
                progress.report(0, 0, "");
                let reader = crate::container::ContainerReader::open(&path, &credentials)?;
                reader.verify()?;
                Ok(JobOutput::Verified(reader.total_data_pages()))
            },
        ));
    }

    fn start_defrag(&mut self) {
        if self.job.is_some() || !self.ensure_unmounted() {
            return;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        session.suspend_lock();
        let path = session.path().to_path_buf();
        let credentials = session.credentials().clone();
        self.job = Some(job::ActiveJob::spawn(
            ts!("gui.job.defrag"),
            0,
            move |progress| {
                let stats = crate::container::defrag::defragment_with_progress(
                    &path,
                    &credentials,
                    true,
                    &|done, total| progress.report(done, total, ""),
                )?;
                Ok(JobOutput::Defrag {
                    files: stats.files_moved,
                })
            },
        ));
    }

    fn start_recover(&mut self) {
        if self.job.is_some() {
            return;
        }
        let Some(output) = self.with_native_modal(|| rfd::FileDialog::new().pick_folder()) else {
            return;
        };
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let path = session.path().to_path_buf();
        let credentials = session.credentials().clone();
        self.job = Some(job::ActiveJob::spawn(
            ts!("gui.job.recover"),
            0,
            move |progress| {
                let reader = crate::container::ContainerReader::open(&path, &credentials)?;
                let report = crate::app::recovery::extract_reader(
                    &reader,
                    &output,
                    true,
                    &|done, total| progress.report(done, total, ""),
                )?;
                Ok(JobOutput::Recovered {
                    extracted: report.extracted(),
                    failed: report.failed(),
                    skipped: report.skipped(),
                })
            },
        ));
    }

    fn poll_job(&mut self) {
        let Some(result) = self.job.as_ref().and_then(|job| job.poll()) else {
            return;
        };
        self.job = None;
        match result {
            Ok(JobOutput::Add(result)) => self.finish_add(result),
            Ok(JobOutput::Extracted { count, removed }) => {
                if let Some(session) = self.session.as_mut() {
                    actions::cleanup_symlinks_for_removed(session, &removed);
                }
                self.clear_selection();
                self.refresh();
                #[cfg(feature = "mount")]
                self.sync_mounted_vfs();
                self.set_status(if removed.is_empty() {
                    ts!("gui.status.extracted_batch", count = count)
                } else {
                    ts!("gui.status.extracted_moved_batch", count = count)
                });
            }
            Ok(JobOutput::Deleted(removed)) => {
                if let Some(session) = self.session.as_mut() {
                    actions::cleanup_symlinks_for_removed(session, &removed);
                }
                self.clear_selection();
                self.refresh();
                #[cfg(feature = "mount")]
                self.sync_mounted_vfs();
                self.set_status(ts!("gui.status.deleted", count = removed.len()));
            }
            Ok(JobOutput::Verified(pages)) => {
                self.set_status(ts!("gui.status.verified", pages = pages));
            }
            Ok(JobOutput::Defrag { files }) => {
                if let Some(session) = self.session.as_mut()
                    && let Err(error) = session.resume_lock()
                {
                    self.set_status(ts!("gui.status.error", error = error.localized()));
                    self.lock();
                    return;
                }
                self.refresh();
                self.set_status(ts!("gui.status.defrag_done", files = files));
            }
            Ok(JobOutput::Recovered {
                extracted,
                failed,
                skipped,
            }) => self.set_status(ts!(
                "gui.status.recovered",
                extracted = extracted,
                failed = failed,
                skipped = skipped
            )),
            Ok(JobOutput::Transferred { count, copied }) => {
                self.clear_selection();
                self.refresh();
                #[cfg(feature = "mount")]
                self.sync_mounted_vfs();
                self.set_status(if copied {
                    ts!("gui.status.copied", count = count)
                } else {
                    ts!("gui.status.moved", count = count)
                });
            }
            #[cfg(feature = "mount")]
            Ok(JobOutput::Mounted { mounted, container }) => self.finish_mount(mounted, container),
            Err(error) => {
                if let Some(session) = self.session.as_mut() {
                    let _ = session.resume_lock();
                }
                let mut message = ts!("gui.status.error", error = error.localized());
                if let Some(hint) = crate::mount_hints::supplemental_guidance(&error) {
                    message.push('\n');
                    message.push_str(&hint);
                }
                self.set_status(message);
            }
        }
    }

    fn handle_dialog_outcome(&mut self, outcome: DialogOutcome) {
        match outcome {
            DialogOutcome::DeleteConfirm { paths } => self.perform_delete(paths),
            DialogOutcome::MoveOut { paths } => self.start_extract(paths, true),
            DialogOutcome::Transfer {
                copy,
                paths,
                destination,
            } => self.start_transfer(copy, paths, destination),
            DialogOutcome::ChangePassword { password, confirm } => {
                self.change_password(password, confirm)
            }
            DialogOutcome::Grow { size } => self.grow_vault(size),
            DialogOutcome::ExtractProceed { paths, skip_future } => {
                if skip_future {
                    self.settings.set_skip_extract_info(true);
                }
                self.perform_extract(paths);
            }
            DialogOutcome::SymlinkProceed { paths, skip_future } => {
                if skip_future {
                    self.settings.set_skip_symlink_info(true);
                }
                if paths.is_empty() {
                    self.add_destination = None;
                    #[cfg(feature = "mount")]
                    {
                        self.create_symlinks = true;
                    }
                } else {
                    self.add_files(paths);
                }
            }
            DialogOutcome::UnlockRecent {
                password,
                no_password,
            } => {
                self.password.zeroize();
                self.password = password;
                self.no_password = no_password;
                self.open();
                if self.session.is_none() {
                    // Retry in the dialog; never leave a rejected password
                    // sitting in the main form.
                    self.password.zeroize();
                    self.password.clear();
                    self.dialog = Some(ActiveDialog::UnlockRecent {
                        path: self.path_input.clone(),
                        password: String::new(),
                        no_password,
                    });
                }
            }
            DialogOutcome::WritebackResolve {
                container_path,
                resolution,
            } => match resolution {
                ConflictResolution::Overwrite => {
                    if let Some(content) = self.read_cache_content(&container_path) {
                        self.write_back_overwrite(&container_path, &content);
                    }
                }
                ConflictResolution::KeepBoth => {
                    if let Some(content) = self.read_cache_content(&container_path) {
                        self.write_back_keep_both(&container_path, &content);
                    }
                }
                ConflictResolution::Discard => self.discard_cache_entry(&container_path),
            },
            DialogOutcome::Cancelled => self.add_destination = None,
        }
    }

    fn handle(&mut self, event: Event) {
        self.touch();
        match event {
            Event::BrowseVault => {
                if let Some(path) = self.with_native_modal(|| {
                    rfd::FileDialog::new()
                        .add_filter(&ts!("gui.sesame_filter"), &["sesame"])
                        .pick_file()
                }) {
                    self.path_input = path.to_string_lossy().into_owned();
                }
            }
            Event::OpenRecent(path) => {
                self.path_input = path.clone();
                if self.no_password {
                    self.open();
                } else {
                    self.dialog = Some(ActiveDialog::UnlockRecent {
                        path,
                        password: String::new(),
                        no_password: false,
                    });
                }
            }
            Event::Open => self.open(),
            Event::Create => self.choose_create_path(),
            Event::SetNoPassword(enabled) => {
                self.no_password = enabled;
                if enabled {
                    self.password.zeroize();
                    self.password.clear();
                }
            }
            Event::AddFiles => {
                if let Some(paths) = self.with_native_modal(|| rfd::FileDialog::new().pick_files())
                {
                    self.request_add_files(paths);
                }
            }
            Event::AddFolder => {
                if let Some(path) = self.with_native_modal(|| rfd::FileDialog::new().pick_folder())
                {
                    self.request_add_files(vec![path]);
                }
            }
            Event::AddPaths(paths) => self.request_add_files(paths),
            Event::Refresh => self.refresh(),
            Event::Lock => self.lock(),
            Event::EnterDirectory(path) => {
                self.current_dir = path;
                self.clear_selection();
            }
            Event::SelectEntry {
                path,
                ctrl,
                shift,
                ordered,
            } => {
                if self.rename_target.as_deref() != Some(path.as_str()) {
                    self.cancel_rename();
                }
                self.apply_selection(path, ctrl, shift, &ordered);
            }
            Event::Mkdir => {
                let name = self.mkdir_input.trim().to_owned();
                if name.is_empty() {
                    self.set_status(ts!("gui.status.name_required"));
                    return;
                }
                if let Some(session) = self.session.as_ref() {
                    let result = {
                        #[cfg(feature = "mount")]
                        let _guard = mount_guard(&self.mounted);
                        actions::mkdir(session, &name, &self.current_dir)
                    };
                    match result {
                        Ok(()) => {
                            self.mkdir_input.clear();
                            self.mkdir_open = false;
                            self.refresh();
                        }
                        Err(error) => {
                            self.set_status(ts!("gui.status.error", error = error.localized()))
                        }
                    }
                }
            }
            Event::RemoveSelected => {
                let paths = super::selection::dedupe_delete_paths(&self.selected_paths);
                if !paths.is_empty() {
                    self.dialog = Some(ActiveDialog::DeleteConfirm { paths });
                }
            }
            Event::BeginRename => {
                if self.selected_paths.len() != 1 {
                    self.set_status(ts!("gui.status.rename_requires_single"));
                    return;
                }
                if let Some(path) = self.selected_paths.iter().next().cloned() {
                    self.begin_rename(path);
                }
            }
            Event::CommitRename => self.commit_rename(),
            Event::CancelRename => self.cancel_rename(),
            Event::SelectAll => {
                let paths: Vec<String> = self
                    .browser
                    .node_at(&self.current_dir)
                    .children
                    .values()
                    .map(|child| child.path.clone())
                    .collect();
                self.selected_paths = paths.into_iter().collect();
                self.selection_anchor = self.selected_paths.iter().next().cloned();
            }
            Event::ClearSelection => self.clear_selection(),
            Event::GoUp => {
                if !self.current_dir.is_empty() {
                    let child = self.current_dir.clone();
                    self.current_dir = self
                        .current_dir
                        .rsplit_once('/')
                        .map(|(parent, _)| parent.to_owned())
                        .unwrap_or_default();
                    // Land on the folder we just left so arrow keys continue
                    // from a sensible place.
                    self.selected_paths.clear();
                    self.selected_paths.insert(child.clone());
                    self.selection_anchor = Some(child);
                }
            }
            Event::ExtractSelected => {
                let paths: Vec<String> = self.selected_paths.iter().cloned().collect();
                if !paths.is_empty() {
                    self.begin_extract(paths);
                }
            }
            Event::BeginCopy => self.begin_transfer(true),
            Event::BeginMove => self.begin_transfer(false),
            Event::BeginMoveOut => {
                let paths: Vec<String> = self.selected_paths.iter().cloned().collect();
                if !paths.is_empty() {
                    self.dialog = Some(ActiveDialog::MoveOut { paths });
                }
            }
            Event::VerifyVault => self.start_verify(),
            Event::DefragVault => self.start_defrag(),
            Event::RecoverVault => self.start_recover(),
            Event::BeginChangePassword => {
                self.dialog = Some(ActiveDialog::ChangePassword {
                    password: String::new(),
                    confirm: String::new(),
                });
            }
            Event::BeginGrow => {
                self.dialog = Some(ActiveDialog::Grow { size: "1GB".into() });
            }
            Event::OpenSelected => {
                if self.selected_paths.len() != 1 {
                    self.set_status(ts!("gui.status.open_requires_single"));
                    return;
                }
                let Some(path) = self.selected_paths.iter().next().cloned() else {
                    return;
                };
                if self
                    .entries
                    .iter()
                    .any(|entry| entry.path == path && entry.is_dir)
                {
                    self.set_status(ts!("gui.status.open_requires_file"));
                    return;
                }
                self.open_in_cache(&path);
            }
            #[cfg(feature = "mount")]
            Event::Mount => self.mount_vault(),
            #[cfg(feature = "mount")]
            Event::Unmount => self.unmount_vault(),
            #[cfg(feature = "mount")]
            Event::SetCreateSymlinks(enabled) => {
                if enabled && !self.settings.hints.skip_symlink_info {
                    self.dialog = Some(ActiveDialog::SymlinkInfo {
                        paths: Vec::new(),
                        dont_show_again: false,
                    });
                } else {
                    self.create_symlinks = enabled;
                }
            }
            Event::ToggleTheme => {
                self.theme = self.theme.toggle();
                self.settings.set_theme(self.theme);
            }
            Event::SetTheme(theme) => {
                self.theme = theme;
                self.settings.set_theme(theme);
            }
            Event::ShowSettings => self.show_settings = true,
            Event::Back => self.show_settings = false,
            Event::SetAutoLockMinutes(minutes) => {
                let minutes = minutes.clamp(1, 60);
                self.auto_lock_minutes = minutes;
                self.auto_lock.set_minutes(minutes);
                self.settings.set_auto_lock(self.auto_lock_enabled, minutes);
            }
            Event::SetAutoLockEnabled(enabled) => {
                self.auto_lock_enabled = enabled;
                self.auto_lock.set_enabled(enabled);
                self.settings.set_auto_lock(enabled, self.auto_lock_minutes);
            }
            Event::SetAntiCapture(enabled) => {
                self.anti_capture_enabled = enabled;
                self.settings.set_anti_capture(enabled);
            }
            Event::ToggleNotifications => self.toggle_notifications(),
            Event::ClearNotifications => self.clear_notifications(),
            #[cfg(feature = "mount")]
            Event::OpenMountLocation => {
                if let Some(mounted) = self.mounted.as_ref() {
                    let point = mounted.mount_point().to_path_buf();
                    if let Err(error) = super::open_external::open_path(&point) {
                        self.set_status(ts!("gui.status.error", error = error.localized()));
                    }
                }
            }
            Event::SetLocale(locale) => {
                self.locale = locale;
                self.settings.set_locale(locale);
                crate::i18n::init(Some(locale));
            }
        }
    }

    pub(crate) fn stage_add(&mut self, dir: String) {
        self.add_destination = Some(dir);
    }

    pub(crate) fn extract_one(&mut self, path: &str) {
        if self.job.is_some() {
            return;
        }
        self.begin_extract(vec![path.to_owned()]);
    }

    pub(crate) fn unlock_from_flyout(&mut self) {
        let path = self.flyout.vault_path.trim().to_owned();
        if path.is_empty() {
            self.set_status(ts!(
                "gui.status.error",
                error = ts!("gui.tray.flyout.no_vault")
            ));
            return;
        }
        self.path_input = path;
        self.no_password = self.flyout.no_password;
        self.password.zeroize();
        self.password = std::mem::take(&mut self.flyout.password);
        self.open();
        if self.session.is_none() {
            self.flyout.password = std::mem::take(&mut self.password);
        }
    }

    pub(crate) fn dismiss_flyout(&mut self, ctx: &egui::Context) {
        if !self.flyout.open {
            return;
        }
        self.flyout.shut();
        if !self.hidden_to_tray {
            ctx.send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Focus);
        }
    }

    fn open_flyout(&mut self, anchor: super::tray::TrayAnchor) {
        self.flyout.open_with(anchor);
        if self.is_unlocked() && self.flyout.dir.is_empty() {
            self.flyout.dir = self.current_dir.clone();
        }
        if !self.is_unlocked() && self.flyout.vault_path.is_empty() {
            if !self.path_input.is_empty() {
                self.flyout.vault_path = self.path_input.clone();
            } else if let Some(recent) = self.recent.entries.first() {
                self.flyout.vault_path = recent.path.display().to_string();
            }
        }
        if !self.is_unlocked() {
            self.flyout.focus_password = true;
        }
        self.touch();
    }

    pub(crate) fn with_native_modal<T>(&mut self, f: impl FnOnce() -> T) -> T {
        let _guard = self.flyout.begin_modal();
        f()
    }

    fn present_flyout(&mut self, ctx: &egui::Context) {
        // Keep the native window alive after the first frame so a click only
        // moves and paints it. Creating the GL surface on the click is the
        // cold start.
        if self.tray.is_none() || self.exit_requested {
            return;
        }
        let open = self.flyout.open;
        let parent_ppp = ctx.input(|input| input.viewport().native_pixels_per_point.unwrap_or(1.0));
        let ppp = self.flyout.scale().unwrap_or(parent_ppp);
        let screen = ctx.input(|input| input.viewport().monitor_size);
        let size = super::flyout::window_size();
        // Off-screen, but still visible on Windows, so a tray click can wake
        // this window without the hidden-root repaint throttle.
        let origin = if open {
            let card = super::flyout::place_flyout(
                &self.flyout.anchor,
                ppp,
                screen,
                egui::vec2(super::flyout::CARD_WIDTH, super::flyout::CARD_HEIGHT),
            );
            super::flyout::window_origin(card)
        } else {
            egui::pos2(-10000.0, -10000.0)
        };
        let builder = egui::ViewportBuilder::default()
            .with_title(super::flyout::WINDOW_TITLE)
            .with_inner_size(size)
            .with_min_inner_size(size)
            .with_max_inner_size(size)
            .with_position(origin)
            .with_resizable(false)
            .with_decorations(false)
            .with_always_on_top()
            .with_taskbar(false)
            .with_drag_and_drop(true)
            .with_mouse_passthrough(!open)
            .with_close_button(false)
            .with_minimize_button(false)
            .with_maximize_button(false)
            // Never toggle this: egui recreates the window when it changes,
            // which is the cold start we are avoiding.
            .with_active(false)
            .with_visible(open || cfg!(windows))
            .with_window_type(egui::X11WindowType::PopupMenu)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_titlebar_buttons_shown(false)
            .with_has_shadow(false);
        #[cfg(not(windows))]
        let builder = builder.with_transparent(true);
        ctx.show_viewport_immediate(super::flyout::viewport_id(), builder, |ui, _class| {
            super::flyout::round_window_frame();
            if !self.flyout.open {
                return;
            }
            let child_ppp =
                ui.input(|input| input.viewport().native_pixels_per_point.unwrap_or(ppp));
            if self.flyout.consume_window_focus() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            super::flyout::show(self, ui);
            if self.flyout.open
                && let Some(outcome) = super::dialog::show(self, ui)
            {
                self.handle_dialog_outcome(outcome);
            }
            if self.flyout.open && (child_ppp - ppp).abs() > 0.05 {
                self.flyout.note_scale(child_ppp);
            }
        });
    }

    fn service_tray(&mut self, ctx: &egui::Context) {
        if !self.tray_attempted {
            self.tray_attempted = true;
            self.tray = super::tray::Tray::install();
        }
        if self.tray.is_none() {
            return;
        }
        super::tray::install_wake(ctx.clone());
        self.sync_tray();
        match super::tray::Tray::poll() {
            Some(super::tray::TrayAction::Show) => {
                self.dismiss_flyout(ctx);
                self.reveal_from_tray(ctx);
            }
            Some(super::tray::TrayAction::ToggleFlyout { anchor }) => {
                if self.flyout.open {
                    self.dismiss_flyout(ctx);
                } else if !self.flyout.swallows_tray_reopen() {
                    self.open_flyout(anchor);
                }
            }
            Some(super::tray::TrayAction::AddFiles) => self.add_from_tray(ctx),
            Some(super::tray::TrayAction::OpenMount) => self.open_mount_from_tray(),
            Some(super::tray::TrayAction::Lock) => {
                if self.is_unlocked() {
                    self.lock();
                }
            }
            Some(super::tray::TrayAction::Quit) => {
                self.dismiss_flyout(ctx);
                self.quit_from_tray(ctx);
            }
            None => self.hide_to_tray_if_dismissed(ctx),
        }
        if !self.exit_requested {
            self.sync_tray();
        }
    }

    fn sync_tray(&mut self) {
        #[cfg(feature = "mount")]
        let mounted = self.mounted.is_some();
        #[cfg(not(feature = "mount"))]
        let mounted = false;
        let unlocked = self.is_unlocked();
        let Some(tray) = self.tray.as_mut() else {
            return;
        };
        let tooltip = if mounted {
            ts!("gui.tray.tooltip_mounted")
        } else if unlocked {
            ts!("gui.tray.tooltip_unlocked")
        } else {
            ts!("gui.tray.tooltip_locked")
        };
        tray.sync(&super::tray::TrayView {
            unlocked,
            mounted,
            show: ts!("gui.tray.show"),
            add: ts!("gui.browser.add_files"),
            open_mount: ts!("gui.browser.open_mount_location"),
            lock: ts!("gui.browser.lock"),
            quit: ts!("gui.tray.quit"),
            tooltip,
        });
    }

    fn add_from_tray(&mut self, ctx: &egui::Context) {
        if !self.is_unlocked() || self.job.is_some() {
            return;
        }
        let Some(paths) = self.with_native_modal(|| rfd::FileDialog::new().pick_files()) else {
            return;
        };
        let waiting_for_dialog = self.dialog.is_some();
        self.request_add_files(paths);
        if !waiting_for_dialog && self.dialog.is_some() {
            self.reveal_from_tray(ctx);
        }
    }

    pub(crate) fn open_mount_from_tray(&mut self) {
        #[cfg(feature = "mount")]
        if let Some(mounted) = self.mounted.as_ref() {
            let point = mounted.mount_point().to_path_buf();
            if let Err(error) = super::open_external::open_path(&point) {
                self.set_status(ts!("gui.status.error", error = error.localized()));
            }
        }
    }

    fn hide_to_tray_if_dismissed(&mut self, ctx: &egui::Context) {
        if self.exit_requested || self.tray.is_none() || self.hidden_to_tray {
            return;
        }
        let (close, minimized) = ctx.input(|input| {
            let viewport = input.viewport();
            (
                viewport.close_requested(),
                viewport.minimized.unwrap_or(false),
            )
        });
        if self.suppress_minimize {
            if !minimized {
                self.suppress_minimize = false;
            } else if !close {
                return;
            }
        }
        if !close && !minimized {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.hidden_to_tray = true;
        self.suppress_minimize = false;
    }

    pub(crate) fn reveal_from_tray(&mut self, ctx: &egui::Context) {
        self.hidden_to_tray = false;
        self.suppress_minimize = true;
        // The tray panel is a second viewport. Commands must name the root
        // window; otherwise Visible/Focus land on the panel and the main
        // window stays hidden.
        let root = egui::ViewportId::ROOT;
        ctx.send_viewport_cmd_to(root, egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd_to(root, egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd_to(root, egui::ViewportCommand::Focus);
        self.touch();
        ctx.request_repaint_of(root);
    }

    fn quit_from_tray(&mut self, ctx: &egui::Context) {
        self.exit_requested = true;
        self.hidden_to_tray = false;
        self.lock();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.service_tray(ctx);
        if self.job.is_some() {
            self.touch();
            self.poll_job();
            ctx.request_repaint_after(Duration::from_millis(80));
        }
        // A hidden main window pauses auto-lock. The tray panel is a visible
        // surface, so idle time counts while it is open.
        let surface_open = !self.hidden_to_tray || self.flyout.open;
        if !surface_open {
            self.touch();
        } else if self.is_unlocked() && self.job.is_none() && self.auto_lock.expired() {
            self.set_status(ts!("gui.status.auto_locked"));
            self.lock();
            ctx.request_repaint();
        }
        if self.is_unlocked() {
            self.poll_cache(ctx);
        }
        self.present_flyout(ctx);
        if self.tray.is_some() && !self.exit_requested {
            let wait = if self.flyout.open {
                80
            } else if self.hidden_to_tray {
                200
            } else {
                400
            };
            ctx.request_repaint_after(Duration::from_millis(wait));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let enabled = self.anti_capture_enabled && self.is_unlocked();
        anti_capture::apply(ui.ctx(), enabled, &mut self.anti_capture_applied);

        // Cross-fade between palettes; `animate_value_with_time` keeps
        // requesting repaints until the blend settles.
        let blend = ui.ctx().animate_value_with_time(
            egui::Id::new("theme_blend"),
            self.theme.blend_target(),
            0.35,
        );
        self.palette = Palette::blend(&Palette::CAVE, &Palette::PARCHMENT, blend);
        self.palette.apply(ui.ctx());

        // Any real input counts as activity for the idle timer, not just
        // clicks that reach an action: scrolling or reading with the mouse
        // moving must not auto-lock the vault mid-use.
        if self.is_unlocked() {
            let active = ui.ctx().input(|input| {
                input.pointer.any_down()
                    || input.pointer.delta() != egui::Vec2::ZERO
                    || input.smooth_scroll_delta != egui::Vec2::ZERO
                    || !input.events.is_empty()
            });
            if active {
                self.touch();
            }
        }
        let mut locale_changed = false;
        let events = views::show(self, ui);
        if self.job.is_none() {
            for event in events {
                if matches!(event, Event::SetLocale(_)) {
                    locale_changed = true;
                }
                self.handle(event);
            }
            if !self.flyout.open
                && let Some(outcome) = super::dialog::show(self, ui)
            {
                self.handle_dialog_outcome(outcome);
            }
        }
        if let Some(job) = &self.job {
            job::overlay(ui, job, self.palette);
        }
        if locale_changed {
            super::fonts::install_fonts(ui.ctx(), self.locale);
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Title(ts!("gui.window_title")));
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.lock();
        self.password.zeroize();
        self.password.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Locale;

    #[test]
    fn gui_visible_strings_are_translated() {
        for locale in [Locale::En, Locale::ZhCn, Locale::Ja] {
            crate::i18n::init(Some(locale));
            let common = crate::ts!("common.error_prefix");
            let title = crate::ts!("gui.locked.title");
            assert_ne!(common, "common.error_prefix", "locale={locale:?}");
            assert_ne!(
                title, "gui.locked.title",
                "locale={locale:?}, common={common}"
            );
        }
    }

    #[test]
    fn path_input_is_persistent_state() {
        let mut app = App::default();
        app.path_input.push_str("C:\\vaults\\private.sesame");
        assert_eq!(app.path_input, "C:\\vaults\\private.sesame");
    }
}
