//! Global shortcuts for the note window. Registered only while a vault is unlocked.
//!
//! The chords are delivered by the OS, so they do not depend on the main window
//! being visible and they are not typed into whatever else is focused.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::Mutex;

static PRESSED: Mutex<Vec<u32>> = Mutex::new(Vec::new());
static WAKE: std::sync::Once = std::sync::Once::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoteHotkey {
    New,
    Reopen,
}

pub(crate) struct NoteHotkeys {
    manager: Option<GlobalHotKeyManager>,
    new_note: HotKey,
    reopen: HotKey,
    new_on: bool,
    reopen_on: bool,
    new_failed: bool,
    reopen_failed: bool,
    /// The hidden hotkey window could not be created. Buttons still work.
    give_up: bool,
}

impl NoteHotkeys {
    pub(crate) fn new() -> Self {
        Self {
            manager: None,
            new_note: HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyN),
            reopen: HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyO),
            new_on: false,
            reopen_on: false,
            new_failed: false,
            reopen_failed: false,
            give_up: false,
        }
    }

    /// Wake the interface when a chord arrives, including while the main window is hidden.
    pub(crate) fn install_wake(ctx: egui::Context) {
        WAKE.call_once(|| {
            GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
                if event.state == HotKeyState::Pressed {
                    if let Ok(mut pressed) = PRESSED.lock() {
                        pressed.push(event.id);
                    }
                    ctx.request_repaint();
                }
            }));
        });
    }

    pub(crate) fn set_armed(&mut self, armed: bool) {
        if !armed {
            if let Some(manager) = self.manager.as_ref() {
                if self.new_on {
                    let _ = manager.unregister(self.new_note);
                }
                if self.reopen_on {
                    let _ = manager.unregister(self.reopen);
                }
            }
            self.new_on = false;
            self.reopen_on = false;
            self.new_failed = false;
            self.reopen_failed = false;
            self.give_up = false;
            return;
        }
        if self.give_up {
            return;
        }
        if self.manager.is_none() {
            match GlobalHotKeyManager::new() {
                Ok(manager) => self.manager = Some(manager),
                Err(_) => {
                    self.give_up = true;
                    return;
                }
            }
        }
        let Some(manager) = self.manager.as_ref() else {
            self.give_up = true;
            return;
        };
        if !self.new_on && !self.new_failed {
            match manager.register(self.new_note) {
                Ok(()) => self.new_on = true,
                Err(_) => self.new_failed = true,
            }
        }
        if !self.reopen_on && !self.reopen_failed {
            match manager.register(self.reopen) {
                Ok(()) => self.reopen_on = true,
                Err(_) => self.reopen_failed = true,
            }
        }
    }

    pub(crate) fn take(&self) -> Vec<NoteHotkey> {
        let Ok(mut pressed) = PRESSED.lock() else {
            return Vec::new();
        };
        let new_id = self.new_note.id();
        let reopen_id = self.reopen.id();
        let mut hits = Vec::new();
        for id in pressed.drain(..) {
            if id == new_id {
                hits.push(NoteHotkey::New);
            } else if id == reopen_id {
                hits.push(NoteHotkey::Reopen);
            }
        }
        hits
    }
}
