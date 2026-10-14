//! Idle timeout that locks the vault automatically.

use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct AutoLock {
    timeout: Duration,
    last_activity: Instant,
    enabled: bool,
}

impl AutoLock {
    pub fn new(minutes: u32) -> Self {
        Self {
            timeout: Duration::from_secs(minutes.max(1) as u64 * 60),
            last_activity: Instant::now(),
            enabled: true,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if enabled {
            self.touch();
        }
    }

    pub fn set_minutes(&mut self, minutes: u32) {
        self.timeout = Duration::from_secs(minutes.max(1) as u64 * 60);
    }

    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    pub fn expired(&self) -> bool {
        self.enabled && self.last_activity.elapsed() >= self.timeout
    }
}
