//! Session-scoped plaintext cache for "open in external app".
//!
//! When a vault file is opened without a mount, it is decrypted into a
//! per-session cache directory (`$TMP/closed-sesame/<uuid>/`). The cache file
//! is writable; a filesystem watcher detects edits, and after a short
//! quiescence window the content is written back into the vault with the
//! writer's two-phase commit. On lock/exit every dirty file is flushed and
//! the whole cache directory is wiped.
//!
//! "File closed" cannot be detected portably (openers like `xdg-open`
//! detach immediately), so a settle window is used as a proxy: no writes for
//! [`SETTLE_WINDOW`] means the editor is considered idle and the content is
//! synced. Final cleanup always happens on lock/exit.

use crate::container::crc32;
use crate::error::{Result, SesameError};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Quiescence window after the last filesystem event before a cache file is
/// considered "closed" and written back to the vault.
pub(crate) const SETTLE_WINDOW: Duration = Duration::from_millis(1500);

/// How the cache session should resolve a vault-side conflict on write-back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConflictResolution {
    /// Overwrite the vault entry with the cache content.
    Overwrite,
    /// Keep both: write the cache content as a new `*.conflict-<ts>` entry.
    KeepBoth,
    /// Drop the cache edits; the vault entry wins.
    Discard,
}

/// One vault file currently mirrored into the cache directory.
#[derive(Debug, Clone)]
pub(crate) struct CacheEntry {
    /// Path inside the vault (e.g. `docs/report.pdf`).
    pub(crate) container_path: String,
    /// Plaintext mirror on disk.
    pub(crate) cache_path: PathBuf,
    /// `modified_at` of the vault entry at extraction time (conflict anchor).
    pub(crate) vault_modified_at: u64,
    /// CRC32 of the content at the last known sync point.
    pub(crate) baseline_crc32: u32,
    /// Last filesystem event not yet written back.
    dirty_since: Option<Instant>,
    /// A conflict dialog is open for this entry; suppress further syncs.
    pub(crate) awaiting_decision: bool,
}

impl CacheEntry {
    pub(crate) fn new(
        container_path: String,
        cache_path: PathBuf,
        vault_modified_at: u64,
        baseline_crc32: u32,
    ) -> Self {
        Self {
            container_path,
            cache_path,
            vault_modified_at,
            baseline_crc32,
            dirty_since: None,
            awaiting_decision: false,
        }
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty_since.is_some()
    }

    /// Re-anchor after a successful write-back.
    pub(crate) fn mark_synced(&mut self, vault_modified_at: u64, content_crc32: u32) {
        self.vault_modified_at = vault_modified_at;
        self.baseline_crc32 = content_crc32;
        self.dirty_since = None;
        self.awaiting_decision = false;
    }
}

/// Outcome of classifying a settled (idle) cache file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WritebackPlan {
    /// Content unchanged since the last sync; nothing to do.
    Unchanged,
    /// Vault entry unchanged since extraction; safe to overwrite.
    Overwrite,
    /// Vault entry changed since extraction; user must decide.
    Conflict,
    /// The vault entry vanished; treat as conflict (user decides).
    MissingFromVault,
}

/// Decide what to do with a settled cache file.
pub(crate) fn plan_writeback(
    entry: &CacheEntry,
    content: &[u8],
    current_vault_modified_at: Option<u64>,
) -> WritebackPlan {
    if crc32(content) == entry.baseline_crc32 {
        return WritebackPlan::Unchanged;
    }
    match current_vault_modified_at {
        None => WritebackPlan::MissingFromVault,
        Some(modified_at) if modified_at == entry.vault_modified_at => WritebackPlan::Overwrite,
        Some(_) => WritebackPlan::Conflict,
    }
}

pub(crate) struct CacheSession {
    dir: PathBuf,
    entries: Vec<CacheEntry>,
    /// Cache path → index into `entries`, kept in sync on register/remove.
    by_path: HashMap<PathBuf, usize>,
    receiver: Receiver<notify::Result<notify::Event>>,
    _watcher: RecommendedWatcher,
    wiped: bool,
}

impl CacheSession {
    pub(crate) fn start() -> Result<Self> {
        let dir = std::env::temp_dir()
            .join("closed-sesame")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&dir)?;
        restrict_dir_permissions(&dir);
        let (sender, receiver) = channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })
        .map_err(|error| SesameError::Platform(format!("cannot watch cache dir: {error}")))?;
        watcher
            .watch(&dir, RecursiveMode::Recursive)
            .map_err(|error| SesameError::Platform(format!("cannot watch cache dir: {error}")))?;
        Ok(Self {
            dir,
            entries: Vec::new(),
            by_path: HashMap::new(),
            receiver,
            _watcher: watcher,
            wiped: false,
        })
    }

    #[cfg(test)]
    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn entries(&self) -> &[CacheEntry] {
        &self.entries
    }

    pub(crate) fn entry(&self, container_path: &str) -> Option<&CacheEntry> {
        self.entries
            .iter()
            .find(|entry| entry.container_path == container_path)
    }

    pub(crate) fn entry_mut(&mut self, container_path: &str) -> Option<&mut CacheEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.container_path == container_path)
    }

    /// Allocate a fresh, timestamp-prefixed cache file name for a vault path.
    pub(crate) fn fresh_cache_path(&self, container_path: &str) -> PathBuf {
        let name = container_path.rsplit('/').next().unwrap_or(container_path);
        let stamped = format!("{}-{name}", timestamp_label(now_secs()));
        self.dir.join(stamped)
    }

    pub(crate) fn register(&mut self, entry: CacheEntry) {
        if let Some(index) = self.by_path.get(&entry.cache_path).copied() {
            self.entries[index] = entry;
            return;
        }
        self.by_path
            .insert(entry.cache_path.clone(), self.entries.len());
        self.entries.push(entry);
    }

    /// Drain watcher events and mark matching entries dirty.
    pub(crate) fn poll_events(&mut self) {
        while let Ok(event) = self.receiver.try_recv() {
            let Ok(event) = event else {
                continue;
            };
            if !matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any
            ) {
                continue;
            }
            for path in event.paths {
                if let Some(&index) = self.by_path.get(&path) {
                    if let Some(entry) = self.entries.get_mut(index) {
                        if !entry.awaiting_decision {
                            entry.dirty_since = Some(Instant::now());
                        }
                    }
                }
            }
        }
    }

    /// Container paths whose edits have settled and are due for write-back.
    /// Marks them non-dirty so a failing write-back is not retried in a hot
    /// loop; failures re-arm on the next filesystem event.
    pub(crate) fn take_settled(&mut self) -> Vec<String> {
        let now = Instant::now();
        let mut settled = Vec::new();
        for entry in &mut self.entries {
            if entry.awaiting_decision {
                continue;
            }
            if let Some(since) = entry.dirty_since {
                if now.duration_since(since) >= SETTLE_WINDOW {
                    entry.dirty_since = None;
                    settled.push(entry.container_path.clone());
                }
            }
        }
        settled
    }

    /// Mark every entry dirty, used for the final flush on lock/exit.
    pub(crate) fn mark_all_dirty(&mut self) {
        for entry in &mut self.entries {
            if entry.cache_path.is_file() {
                entry.dirty_since = Some(Instant::now());
            }
        }
    }

    pub(crate) fn remove(&mut self, container_path: &str) {
        if let Some(position) = self
            .entries
            .iter()
            .position(|entry| entry.container_path == container_path)
        {
            let entry = self.entries.remove(position);
            self.by_path.remove(&entry.cache_path);
            self.by_path = self
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| (entry.cache_path.clone(), index))
                .collect();
        }
    }

    /// Wipe the cache directory (best-effort zero-fill, then delete).
    pub(crate) fn wipe(mut self) {
        self.wipe_inner(&[]);
    }

    /// Wipe everything except the given cache paths, which are preserved on
    /// disk (used when a lock-time write-back failed, so edits survive).
    pub(crate) fn wipe_preserving(mut self, keep: &[PathBuf]) {
        self.wipe_inner(keep);
    }

    fn wipe_inner(&mut self, keep: &[PathBuf]) {
        for entry in &self.entries {
            if !keep.contains(&entry.cache_path) {
                zero_and_remove(&entry.cache_path);
            }
        }
        if keep.is_empty() {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
        self.wiped = true;
    }
}

impl Drop for CacheSession {
    fn drop(&mut self) {
        if self.wiped {
            return;
        }
        for entry in &self.entries {
            zero_and_remove(&entry.cache_path);
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

pub(crate) fn zero_and_remove(path: &Path) {
    if let Ok(metadata) = path.metadata() {
        if metadata.is_file() {
            if let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(path) {
                use std::io::Write;
                let zeros = vec![0u8; metadata.len().min(16 * 1024 * 1024) as usize];
                let _ = file.write_all(&zeros);
                let _ = file.sync_all();
            }
        }
    }
    let _ = std::fs::remove_file(path);
}

#[cfg(unix)]
fn restrict_dir_permissions(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn restrict_dir_permissions(_dir: &Path) {}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// `yyyymmdd-hhmmss` in UTC, no external date crate.
pub(crate) fn timestamp_label(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let secs_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60,
    )
}

/// `yyyy-mm-dd` in UTC for a unix timestamp.
pub(crate) fn date_label(secs: u64) -> String {
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Howard Hinnant's civil-from-days algorithm.
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Build a conflict-copy container path: `dir/stem.conflict-<ts>.<ext>`.
pub(crate) fn conflict_copy_path(container_path: &str, secs: u64) -> String {
    let (dir, name) = match container_path.rsplit_once('/') {
        Some((dir, name)) => (Some(dir), name),
        None => (None, container_path),
    };
    let stamped = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => {
            format!("{stem}.conflict-{}.{ext}", timestamp_label(secs))
        }
        _ => format!("{name}.conflict-{}", timestamp_label(secs)),
    };
    match dir {
        Some(dir) => format!("{dir}/{stamped}"),
        None => stamped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_label_is_utc_civil_time() {
        assert_eq!(timestamp_label(0), "19700101-000000");
        // 1_700_000_000 = 2023-11-14 22:13:20 UTC
        assert_eq!(timestamp_label(1_700_000_000), "20231114-221320");
    }

    #[test]
    fn conflict_copy_path_preserves_dir_and_extension() {
        assert_eq!(
            conflict_copy_path("docs/report.pdf", 0),
            "docs/report.conflict-19700101-000000.pdf"
        );
        assert_eq!(
            conflict_copy_path("plain", 0),
            "plain.conflict-19700101-000000"
        );
    }

    #[test]
    fn plan_writeback_classifies_states() {
        let entry = CacheEntry {
            container_path: "a.txt".into(),
            cache_path: PathBuf::from("/tmp/a"),
            vault_modified_at: 100,
            baseline_crc32: crc32(b"old"),
            dirty_since: None,
            awaiting_decision: false,
        };
        assert_eq!(
            plan_writeback(&entry, b"old", Some(100)),
            WritebackPlan::Unchanged
        );
        assert_eq!(
            plan_writeback(&entry, b"new", Some(100)),
            WritebackPlan::Overwrite
        );
        assert_eq!(
            plan_writeback(&entry, b"new", Some(200)),
            WritebackPlan::Conflict
        );
        assert_eq!(
            plan_writeback(&entry, b"new", None),
            WritebackPlan::MissingFromVault
        );
    }

    #[test]
    fn cache_session_lifecycle_wipes_dir() {
        let mut session = CacheSession::start().unwrap();
        let dir = session.dir().to_path_buf();
        let file = dir.join("test.txt");
        std::fs::write(&file, b"secret").unwrap();
        session.register(CacheEntry {
            container_path: "test.txt".into(),
            cache_path: file.clone(),
            vault_modified_at: 0,
            baseline_crc32: crc32(b"secret"),
            dirty_since: None,
            awaiting_decision: false,
        });
        session.wipe();
        assert!(!dir.exists());
    }
}
