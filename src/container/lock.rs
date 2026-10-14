//! Cross-process exclusive lock for a container.
//!
//! The lock is a sidecar file next to the container (`*.sesame.lock`), not a
//! byte-range lock on the container itself. Windows treats `LockFileEx` as
//! mandatory for every handle, so locking the container would block this
//! process's own reads and writes. The sidecar stays put when defrag replaces
//! the container file.
//!
//! The first acquire in this process takes the OS lock and holds it until the
//! last in-process guard drops. Later acquires in the same process share that
//! lock. Another process fails immediately with [`SesameError::ContainerBusy`].

use crate::error::{Result, SesameError};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

struct Slot {
    /// Owns the OS lock. Dropping the slot unlocks it.
    #[allow(dead_code)]
    file: File,
    refs: usize,
}

static LOCKS: LazyLock<Mutex<HashMap<PathBuf, Slot>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Held for as long as this process wants other processes kept out.
#[derive(Debug)]
pub struct VaultLock {
    path: PathBuf,
}

impl VaultLock {
    pub fn try_acquire(path: &Path) -> Result<Self> {
        let key = path.canonicalize()?;
        let mut locks = LOCKS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(slot) = locks.get_mut(&key) {
            slot.refs += 1;
            return Ok(Self { path: key });
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(sidecar(&key))?;
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err(SesameError::ContainerBusy(key.display().to_string()));
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(error.into()),
        }
        locks.insert(key.clone(), Slot { file, refs: 1 });
        Ok(Self { path: key })
    }
}

impl Drop for VaultLock {
    fn drop(&mut self) {
        let mut locks = LOCKS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let remove = if let Some(slot) = locks.get_mut(&self.path) {
            slot.refs = slot.refs.saturating_sub(1);
            slot.refs == 0
        } else {
            false
        };
        if remove {
            locks.remove(&self.path);
        }
    }
}

fn sidecar(container: &Path) -> PathBuf {
    let mut name = container.as_os_str().to_owned();
    name.push(".lock");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_process_reentry_holds_one_os_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.sesame");
        std::fs::File::create(&path).unwrap();

        let first = VaultLock::try_acquire(&path).unwrap();
        let second = VaultLock::try_acquire(&path).unwrap();
        let outsider = OpenOptions::new()
            .read(true)
            .write(true)
            .open(sidecar(&path.canonicalize().unwrap()))
            .unwrap();
        assert!(matches!(
            outsider.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        drop(second);
        assert!(matches!(
            outsider.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        drop(first);
        outsider.try_lock().unwrap();
        outsider.unlock().unwrap();
    }

    #[test]
    fn acquire_fails_when_another_handle_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.sesame");
        std::fs::File::create(&path).unwrap();
        let key = path.canonicalize().unwrap();
        let held = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(sidecar(&key))
            .unwrap();
        held.try_lock().unwrap();
        let error = VaultLock::try_acquire(&path).unwrap_err();
        assert!(matches!(error, SesameError::ContainerBusy(_)));
        held.unlock().unwrap();
    }
}
