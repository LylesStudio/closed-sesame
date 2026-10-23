//! System tray icon. Closing or minimizing the window hides it here so a
//! mount can keep running. The process exits only from the tray menu.

use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, Once, OnceLock};

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

const SHOW_ID: &str = "closed-sesame.tray.show";
const ADD_ID: &str = "closed-sesame.tray.add";
const MOUNT_ID: &str = "closed-sesame.tray.mount";
const LOCK_ID: &str = "closed-sesame.tray.lock";
const QUIT_ID: &str = "closed-sesame.tray.quit";
const ICON_PX: u32 = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Presence {
    Locked,
    Unlocked,
    Mounted,
}

/// Where the tray icon was when the user clicked it. Coordinates are physical pixels.
/// Linux StatusNotifier leaves both empty; the panel then sits in the screen corner.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TrayAnchor {
    pub(crate) icon: Option<TrayRect>,
    pub(crate) cursor: Option<(f64, f64)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TrayRect {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TrayAction {
    /// Open the main window. A double-click, or the menu's Show item.
    Show,
    /// Left click: open or close the tray panel.
    ToggleFlyout {
        anchor: TrayAnchor,
    },
    AddFiles,
    OpenMount,
    Lock,
    Quit,
}

enum PointerHit {
    Click(TrayAnchor),
    DoubleClick,
}

/// What the tray should show right now. Strings are already localized.
pub(crate) struct TrayView {
    pub unlocked: bool,
    pub mounted: bool,
    pub show: String,
    pub add: String,
    pub open_mount: String,
    pub lock: String,
    pub quit: String,
    pub tooltip: String,
}

pub(crate) struct Tray {
    icon: TrayIcon,
    menu: Menu,
    show: MenuItem,
    add: MenuItem,
    open_mount: MenuItem,
    lock: MenuItem,
    separator: PredefinedMenuItem,
    quit: MenuItem,
    locked_icon: Icon,
    unlocked_icon: Icon,
    mounted_icon: Icon,
    presence: Presence,
    menu_unlocked: bool,
    menu_mounted: bool,
    show_label: String,
    add_label: String,
    open_mount_label: String,
    lock_label: String,
    quit_label: String,
    tooltip: String,
}

impl Tray {
    /// Install the icon. `None` means this desktop has no usable tray, and
    /// the window should close for real.
    pub(crate) fn install() -> Option<Self> {
        let (base, width, height) = base_rgba()?;
        let locked_icon = Icon::from_rgba(locked_rgba(&base), width, height).ok()?;
        let unlocked_icon = Icon::from_rgba(base.clone(), width, height).ok()?;
        let mounted_icon =
            Icon::from_rgba(mounted_rgba(&base, width, height), width, height).ok()?;
        let show = MenuItem::with_id(SHOW_ID, "Show", true, None);
        let add = MenuItem::with_id(ADD_ID, "Add files", true, None);
        let open_mount = MenuItem::with_id(MOUNT_ID, "Open mount", true, None);
        let lock = MenuItem::with_id(LOCK_ID, "Lock", true, None);
        let separator = PredefinedMenuItem::separator();
        let quit = MenuItem::with_id(QUIT_ID, "Quit", true, None);
        let menu = Menu::new();
        menu.append_items(&[&show, &separator, &quit]).ok()?;
        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            // Left click opens the tray panel. The menu stays on right click.
            .with_menu_on_left_click(false)
            .with_menu_on_right_click(true)
            .with_tooltip("ClosedSesame")
            .with_icon(locked_icon.clone())
            .build()
            .ok()?;
        Some(Self {
            icon,
            menu,
            show,
            add,
            open_mount,
            lock,
            separator,
            quit,
            locked_icon,
            unlocked_icon,
            mounted_icon,
            presence: Presence::Locked,
            menu_unlocked: false,
            menu_mounted: false,
            show_label: String::new(),
            add_label: String::new(),
            open_mount_label: String::new(),
            lock_label: String::new(),
            quit_label: String::new(),
            tooltip: String::new(),
        })
    }

    pub(crate) fn sync(&mut self, view: &TrayView) {
        if self.show_label != view.show {
            self.show.set_text(&view.show);
            self.show_label = view.show.clone();
        }
        if self.add_label != view.add {
            self.add.set_text(&view.add);
            self.add_label = view.add.clone();
        }
        if self.open_mount_label != view.open_mount {
            self.open_mount.set_text(&view.open_mount);
            self.open_mount_label = view.open_mount.clone();
        }
        if self.lock_label != view.lock {
            self.lock.set_text(&view.lock);
            self.lock_label = view.lock.clone();
        }
        if self.quit_label != view.quit {
            self.quit.set_text(&view.quit);
            self.quit_label = view.quit.clone();
        }
        if self.tooltip != view.tooltip && self.icon.set_tooltip(Some(&view.tooltip)).is_ok() {
            self.tooltip = view.tooltip.clone();
        }
        let presence = match (view.unlocked, view.mounted) {
            (true, true) => Presence::Mounted,
            (true, false) => Presence::Unlocked,
            (false, _) => Presence::Locked,
        };
        if presence != self.presence {
            let icon = match presence {
                Presence::Locked => &self.locked_icon,
                Presence::Unlocked => &self.unlocked_icon,
                Presence::Mounted => &self.mounted_icon,
            };
            if self.icon.set_icon(Some(icon.clone())).is_ok() {
                self.presence = presence;
            }
        }
        if view.unlocked != self.menu_unlocked
            || (view.unlocked && view.mounted != self.menu_mounted)
        {
            self.rebuild_menu(view.unlocked, view.unlocked && view.mounted);
        }
    }

    fn rebuild_menu(&mut self, unlocked: bool, mounted: bool) {
        let _ = self.menu.remove(&self.show);
        let _ = self.menu.remove(&self.add);
        let _ = self.menu.remove(&self.open_mount);
        let _ = self.menu.remove(&self.lock);
        let _ = self.menu.remove(&self.separator);
        let _ = self.menu.remove(&self.quit);
        let mut items: Vec<&dyn tray_icon::menu::IsMenuItem> = vec![&self.show];
        if unlocked {
            items.push(&self.add);
            if mounted {
                items.push(&self.open_mount);
            }
            items.push(&self.lock);
        }
        items.push(&self.separator);
        items.push(&self.quit);
        if self.menu.append_items(&items).is_ok() {
            self.menu_unlocked = unlocked;
            self.menu_mounted = mounted;
        }
    }

    /// Drain tray clicks and menu selections. A menu choice wins over a click
    /// in the same batch. A double-click in the same batch opens the main
    /// window instead of toggling the panel.
    pub(crate) fn poll() -> Option<TrayAction> {
        let mut hits = Vec::new();
        let mut menus = Vec::new();
        drain_events(&mut hits, &mut menus);
        let menu = menus.into_iter().reduce(|_, menu| menu);
        prefer_menu(resolve_pointer(&hits), menu)
    }
}

struct Inbox {
    icons: Mutex<Receiver<TrayIconEvent>>,
    menus: Mutex<Receiver<MenuEvent>>,
}

static INBOX: OnceLock<Inbox> = OnceLock::new();
static WAKE: Once = Once::new();

/// Tray clicks arrive on a hidden window, not the egui one. Forward them into
/// a channel and wake the event loop immediately, instead of waiting for the
/// hidden main window's next throttled repaint.
pub(crate) fn install_wake(ctx: egui::Context) {
    WAKE.call_once(|| {
        let (icon_tx, icon_rx) = mpsc::channel();
        let (menu_tx, menu_rx) = mpsc::channel();
        let _ = INBOX.set(Inbox {
            icons: Mutex::new(icon_rx),
            menus: Mutex::new(menu_rx),
        });
        let flyout = super::flyout::viewport_id();
        let ctx_icon = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            let wake = matches!(
                event,
                TrayIconEvent::Click { .. } | TrayIconEvent::DoubleClick { .. }
            );
            let _ = icon_tx.send(event);
            if wake {
                ctx_icon.request_repaint_of(flyout);
                ctx_icon.request_repaint();
            }
        }));
        let ctx_menu = ctx;
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = menu_tx.send(event);
            ctx_menu.request_repaint_of(flyout);
            ctx_menu.request_repaint();
        }));
    });
}

fn drain_events(hits: &mut Vec<PointerHit>, menus: &mut Vec<TrayAction>) {
    let mut take_icon = |event: TrayIconEvent| match event {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            position,
            rect,
            ..
        } => hits.push(PointerHit::Click(anchor_of(position, rect))),
        TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => hits.push(PointerHit::DoubleClick),
        _ => {}
    };
    let mut take_menu = |event: MenuEvent| {
        if let Some(action) = menu_action(event.id()) {
            menus.push(action);
        }
    };
    if let Some(inbox) = INBOX.get() {
        if let Ok(icons) = inbox.icons.lock() {
            while let Ok(event) = icons.try_recv() {
                take_icon(event);
            }
        }
        if let Ok(menu) = inbox.menus.lock() {
            while let Ok(event) = menu.try_recv() {
                take_menu(event);
            }
        }
        return;
    }
    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        take_icon(event);
    }
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        take_menu(event);
    }
}

fn anchor_of(position: tray_icon::dpi::PhysicalPosition<f64>, rect: tray_icon::Rect) -> TrayAnchor {
    let icon = if rect.size.width > 0 && rect.size.height > 0 {
        Some(TrayRect {
            x: rect.position.x,
            y: rect.position.y,
            width: f64::from(rect.size.width),
            height: f64::from(rect.size.height),
        })
    } else {
        None
    };
    let cursor = if position.x.abs() > 1.0 || position.y.abs() > 1.0 {
        Some((position.x, position.y))
    } else {
        None
    };
    TrayAnchor { icon, cursor }
}

fn menu_action(id: &tray_icon::menu::MenuId) -> Option<TrayAction> {
    if id == SHOW_ID {
        Some(TrayAction::Show)
    } else if id == ADD_ID {
        Some(TrayAction::AddFiles)
    } else if id == MOUNT_ID {
        Some(TrayAction::OpenMount)
    } else if id == LOCK_ID {
        Some(TrayAction::Lock)
    } else if id == QUIT_ID {
        Some(TrayAction::Quit)
    } else {
        None
    }
}

/// A double-click swallows the preceding single click so the panel does not
/// flash open before the main window.
fn resolve_pointer(hits: &[PointerHit]) -> Option<TrayAction> {
    if hits
        .iter()
        .any(|hit| matches!(hit, PointerHit::DoubleClick))
    {
        return Some(TrayAction::Show);
    }
    hits.iter().rev().find_map(|hit| match hit {
        PointerHit::Click(anchor) => Some(TrayAction::ToggleFlyout {
            anchor: anchor.clone(),
        }),
        PointerHit::DoubleClick => None,
    })
}

fn prefer_menu(pointer: Option<TrayAction>, menu: Option<TrayAction>) -> Option<TrayAction> {
    menu.or(pointer)
}

fn base_rgba() -> Option<(Vec<u8>, u32, u32)> {
    let image = image::load_from_memory(include_bytes!("../../assets/logo-mark.png")).ok()?;
    let image = image.resize(ICON_PX, ICON_PX, image::imageops::FilterType::Lanczos3);
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some((rgba.into_raw(), width, height))
}

fn locked_rgba(base: &[u8]) -> Vec<u8> {
    let mut rgba = base.to_vec();
    for pixel in rgba.chunks_exact_mut(4) {
        if pixel[3] == 0 {
            continue;
        }
        pixel[0] = (u16::from(pixel[0]) * 3 / 5) as u8;
        pixel[1] = (u16::from(pixel[1]) * 3 / 5) as u8;
        pixel[2] = (u16::from(pixel[2]) * 3 / 5) as u8;
        pixel[3] = (u16::from(pixel[3]) * 7 / 10) as u8;
    }
    rgba
}

fn mounted_rgba(base: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut rgba = base.to_vec();
    let radius = (width / 8).max(4);
    let cx = width.saturating_sub(radius + 1);
    let cy = height.saturating_sub(radius + 1);
    fill_circle(
        &mut rgba,
        width,
        height,
        cx,
        cy,
        radius + 1,
        [0x14, 0x11, 0x0A, 255],
    );
    fill_circle(
        &mut rgba,
        width,
        height,
        cx,
        cy,
        radius.saturating_sub(1).max(2),
        [0xF3, 0xE6, 0xB0, 255],
    );
    rgba
}

fn fill_circle(
    rgba: &mut [u8],
    width: u32,
    height: u32,
    cx: u32,
    cy: u32,
    radius: u32,
    color: [u8; 4],
) {
    let r = radius as i32;
    let cx = cx as i32;
    let cy = cy as i32;
    let x0 = (cx - r).max(0) as u32;
    let y0 = (cy - r).max(0) as u32;
    let x1 = ((cx + r) as u32).min(width.saturating_sub(1));
    let y1 = ((cy + r) as u32).min(height.saturating_sub(1));
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as i32 - cx;
            let dy = y as i32 - cy;
            if dx * dx + dy * dy <= r * r {
                let index = ((y * width + x) * 4) as usize;
                rgba[index..index + 4].copy_from_slice(&color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_click_opens_the_window_instead_of_the_panel() {
        let anchor = TrayAnchor {
            icon: Some(TrayRect {
                x: 10.0,
                y: 20.0,
                width: 16.0,
                height: 16.0,
            }),
            cursor: Some((12.0, 22.0)),
        };
        let hits = [PointerHit::Click(anchor.clone()), PointerHit::DoubleClick];
        assert_eq!(resolve_pointer(&hits), Some(TrayAction::Show));
        assert_eq!(
            resolve_pointer(&[PointerHit::Click(anchor.clone())]),
            Some(TrayAction::ToggleFlyout { anchor })
        );
    }

    #[test]
    fn menu_choice_wins_over_a_click() {
        let click = Some(TrayAction::ToggleFlyout {
            anchor: TrayAnchor::default(),
        });
        assert_eq!(
            prefer_menu(click, Some(TrayAction::Lock)),
            Some(TrayAction::Lock)
        );
        assert_eq!(prefer_menu(None, None), None);
    }

    #[test]
    fn tray_states_are_visually_distinct() {
        let (base, width, height) = base_rgba().expect("logo");
        let locked = locked_rgba(&base);
        let mounted = mounted_rgba(&base, width, height);
        assert_eq!(base.len(), (width * height * 4) as usize);
        assert_ne!(base, locked);
        assert_ne!(base, mounted);
        assert_ne!(locked, mounted);
    }
}
