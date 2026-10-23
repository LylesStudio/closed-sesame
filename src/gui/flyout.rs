//! Tray panel. A left click on the icon opens this beside the tray; the main
//! window stays where it was. Right-click keeps the native menu.
//!
//! The panel is a second native viewport: borderless, always on top, and kept
//! out of the taskbar. Once it has been focused, clicking elsewhere closes it
//! at once. A file dialog is not treated as clicking elsewhere.

use super::app::App;
use super::browser::BrowserNode;
use super::icons::{Icon, paint_icon};
use super::theme::{Palette, lerp_color, paint_radial_glow, with_alpha};
use super::tray::{TrayAnchor, TrayRect};
use super::widgets::{
    check_box, chip_button, clickable, flat_icon_button, input_field_sized, primary_button,
    section_label, truncated_galley, vault_display_name,
};
use crate::app::fmt_size;
use egui::{
    Align, FontId, Layout, Pos2, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind, Ui, Vec2,
    pos2, vec2,
};
use std::cmp::Ordering;
use std::time::{Duration, Instant};
use zeroize::Zeroize;

/// Visual size of the panel, matching a compact slice of the main window.
pub(crate) const CARD_WIDTH: f32 = 332.0;
pub(crate) const CARD_HEIGHT: f32 = 512.0;
/// Room around the card for the painted shadow on platforms without a window manager clip.
#[cfg(not(windows))]
pub(crate) const SHADOW_PAD: f32 = 20.0;

pub(crate) const WINDOW_TITLE: &str = "ClosedSesame.tray";

/// Outer size of the native window. Windows rounds the frame itself, so the
/// client area is the card. Elsewhere the card sits inside a transparent margin.
pub(crate) fn window_size() -> Vec2 {
    #[cfg(windows)]
    {
        vec2(CARD_WIDTH, CARD_HEIGHT)
    }
    #[cfg(not(windows))]
    {
        vec2(
            CARD_WIDTH + SHADOW_PAD * 2.0,
            CARD_HEIGHT + SHADOW_PAD * 2.0,
        )
    }
}

pub(crate) fn window_origin(card: Pos2) -> Pos2 {
    #[cfg(windows)]
    {
        card
    }
    #[cfg(not(windows))]
    {
        card - vec2(SHADOW_PAD, SHADOW_PAD)
    }
}

pub(crate) fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("closed-sesame.tray.flyout")
}

const GAP: f32 = 10.0;
const SCREEN_MARGIN: f32 = 8.0;
const ROW_H: f32 = 34.0;
/// Mouse-down on the tray blurs the panel; mouse-up is the click that would
/// open it again. Ignore that reopen for one click's worth of time.
const TRAY_REOPEN_GUARD: Duration = Duration::from_millis(400);

pub(crate) struct FlyoutState {
    pub(crate) open: bool,
    pub(crate) anchor: TrayAnchor,
    pub(crate) password: String,
    pub(crate) no_password: bool,
    pub(crate) vault_path: String,
    pub(crate) dir: String,
    pub(crate) filter: String,
    pub(crate) selected: Option<String>,
    pub(crate) focus_password: bool,
    focus_window: bool,
    /// File dialogs run a nested loop. Blur during that loop is not "clicked outside".
    pub(crate) native_modal: bool,
    /// Focus has arrived at least once since open. Before that, an unfocused
    /// frame is just the window coming up, not the user clicking away.
    seen_focus: bool,
    closed_by_blur_at: Option<Instant>,
    /// The frame after a folder change eats the second half of a double-click,
    /// which would otherwise land on whatever row slid under the cursor.
    suppress_click: bool,
    capture_applied: Option<bool>,
    /// Scale of the panel's own window, once it exists. The tray rect is in
    /// physical pixels; this converts it on the monitor the panel landed on.
    scale: Option<f32>,
}

impl Default for FlyoutState {
    fn default() -> Self {
        Self {
            open: false,
            anchor: TrayAnchor::default(),
            password: String::new(),
            no_password: false,
            vault_path: String::new(),
            dir: String::new(),
            filter: String::new(),
            selected: None,
            focus_password: false,
            focus_window: false,
            native_modal: false,
            seen_focus: false,
            closed_by_blur_at: None,
            suppress_click: false,
            capture_applied: None,
            scale: None,
        }
    }
}

impl Drop for FlyoutState {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

impl FlyoutState {
    pub(crate) fn open_with(&mut self, anchor: TrayAnchor) {
        self.open = true;
        self.anchor = anchor;
        self.seen_focus = false;
        self.closed_by_blur_at = None;
        self.suppress_click = false;
        self.focus_window = true;
        self.capture_applied = None;
    }

    pub(crate) fn consume_window_focus(&mut self) -> bool {
        std::mem::take(&mut self.focus_window)
    }

    pub(crate) fn shut(&mut self) {
        self.open = false;
        self.seen_focus = false;
        self.suppress_click = false;
        self.focus_password = false;
        self.focus_window = false;
        self.capture_applied = None;
        self.password.zeroize();
        self.password.clear();
    }

    /// The panel just closed because it lost focus. The tray click that caused
    /// that blur must not toggle it open again.
    pub(crate) fn note_blur_close(&mut self) {
        self.closed_by_blur_at = Some(Instant::now());
    }

    pub(crate) fn swallows_tray_reopen(&self) -> bool {
        self.closed_by_blur_at
            .is_some_and(|at| at.elapsed() < TRAY_REOPEN_GUARD)
    }

    /// Lock cleared the session. Keep the panel up on the same vault so the
    /// password can be typed again.
    pub(crate) fn on_locked(&mut self, vault: String) {
        self.password.zeroize();
        self.password.clear();
        self.dir.clear();
        self.filter.clear();
        self.selected = None;
        self.suppress_click = false;
        if !vault.is_empty() {
            self.vault_path = vault;
        }
        if self.open {
            self.focus_password = true;
        }
    }

    pub(crate) fn note_scale(&mut self, scale: f32) {
        let changed = self
            .scale
            .is_none_or(|current| (current - scale).abs() > 0.05);
        if scale.is_finite() && scale > 0.1 && changed {
            self.scale = Some(scale);
        }
    }

    pub(crate) fn scale(&self) -> Option<f32> {
        self.scale
    }

    pub(crate) fn begin_modal(&mut self) -> ModalGuard<'_> {
        self.native_modal = true;
        ModalGuard { flyout: self }
    }

    /// Close as soon as focus leaves, after the panel has actually been focused.
    pub(crate) fn should_dismiss(&mut self, focused: Option<bool>, blocked: bool) -> bool {
        if !self.open || blocked || self.native_modal {
            return false;
        }
        if focused == Some(true) {
            self.seen_focus = true;
            return false;
        }
        self.seen_focus && focused == Some(false)
    }
}

pub(crate) struct ModalGuard<'a> {
    flyout: &'a mut FlyoutState,
}

impl Drop for ModalGuard<'_> {
    fn drop(&mut self) {
        self.flyout.native_modal = false;
        // The dialog's own blur must not count as clicking away. Wait until
        // the panel is focused again.
        self.flyout.seen_focus = false;
    }
}

struct Row {
    name: String,
    path: String,
    is_dir: bool,
    meta: String,
}

pub(crate) fn show(app: &mut App, ui: &mut Ui) {
    round_window_frame();
    app.palette.apply(ui.ctx());
    let enabled = app.anti_capture_enabled && app.is_unlocked();
    if app.flyout.capture_applied != Some(enabled) {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::ContentProtected(enabled));
        app.flyout.capture_applied = Some(enabled);
    }

    let screen = ui.max_rect();
    let card = card_rect(screen);
    paint_shell(ui, card, app.palette);

    let inner = card.shrink2(vec2(14.0, 12.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        ui.style_mut().spacing.item_spacing.y = 8.0;
        let active = ui.ctx().input(|input| {
            input.pointer.any_down()
                || input.pointer.delta() != Vec2::ZERO
                || input.smooth_scroll_delta != Vec2::ZERO
                || !input.events.is_empty()
        });
        if active {
            app.touch();
        }
        accept_drop(app, ui);
        header(app, ui);
        if app.is_unlocked() {
            unlocked(app, ui);
        } else {
            locked(app, ui);
        }
        let typing = ui.memory(|mem| mem.focused().is_some());
        let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
        if escape && app.dialog.is_none() {
            let clearing_filter = typing && !app.flyout.filter.is_empty();
            if clearing_filter {
                app.flyout.filter.clear();
            } else {
                app.dismiss_flyout(ui.ctx());
            }
        }
    });

    let focused = ui.input(|input| input.viewport().focused);
    let close_requested = ui.input(|input| input.viewport().close_requested());
    if close_requested {
        app.dismiss_flyout(ui.ctx());
    } else if app.flyout.should_dismiss(focused, app.dialog.is_some()) {
        app.flyout.note_blur_close();
        app.dismiss_flyout(ui.ctx());
    }
}

fn accept_drop(app: &mut App, ui: &mut Ui) {
    let dropped: Vec<_> = ui.ctx().input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect()
    });
    if dropped.is_empty() || app.has_job() {
        return;
    }
    if app.is_unlocked() {
        let dir = app.flyout.dir.clone();
        app.stage_add(dir);
        app.request_add_files(dropped);
    } else {
        app.set_status(ts!("gui.tray.flyout.locked_hint"));
    }
}

fn card_rect(screen: Rect) -> Rect {
    #[cfg(windows)]
    {
        screen
    }
    #[cfg(not(windows))]
    {
        screen.shrink(SHADOW_PAD)
    }
}

/// Same shell as the main window: obsidian (or parchment) fill, hairline
/// border, and the brand's gold edge. Windows clips this to the system corner
/// radius used by the decorated main window. Other desktops paint the radius
/// themselves on a transparent window.
fn paint_shell(ui: &Ui, card: Rect, p: Palette) {
    let radius = p.card_radius();
    #[cfg(not(windows))]
    {
        let shadow = egui::epaint::Shadow {
            offset: [0, 8],
            blur: 18,
            spread: 0,
            color: with_alpha(p.shadow, 0.55),
        };
        ui.painter().add(shadow.as_shape(card, radius));
    }
    #[cfg(windows)]
    {
        let _ = radius;
    }
    let corner = {
        #[cfg(windows)]
        {
            0.0
        }
        #[cfg(not(windows))]
        {
            radius
        }
    };
    ui.painter().rect_filled(card, corner, p.bg);
    paint_radial_glow(
        ui.painter(),
        pos2(card.left() + 36.0, card.top() + 28.0),
        110.0,
        with_alpha(p.accent_glow, if p.is_dark() { 0.8 } else { 0.5 }),
        1.8,
    );
    ui.painter().rect_stroke(
        card.shrink(0.5),
        corner,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
}

/// Give the borderless panel the same corner clip as the decorated main window.
pub(crate) fn round_window_frame() {
    #[cfg(windows)]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static ROUNDED: AtomicBool = AtomicBool::new(false);
        if ROUNDED.load(Ordering::Relaxed) {
            return;
        }
        if apply_windows_round_corners() {
            ROUNDED.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(windows)]
fn apply_windows_round_corners() -> bool {
    use windows::Win32::Graphics::Dwm::{
        DWM_WINDOW_CORNER_PREFERENCE, DWMWA_WINDOW_CORNER_PREFERENCE, DwmSetWindowAttribute,
    };
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    use windows::core::PCWSTR;

    let mut title: Vec<u16> = WINDOW_TITLE.encode_utf16().collect();
    title.push(0);
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) else {
            return false;
        };
        if hwnd.is_invalid() {
            return false;
        }
        // DWMWCP_ROUND: the same preference Windows uses for a normal decorated window.
        let preference = DWM_WINDOW_CORNER_PREFERENCE(2);
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &preference as *const _ as _,
            std::mem::size_of_val(&preference) as u32,
        )
        .is_ok()
    }
}

fn header(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let name = header_name(app);
    let capacity = app.capacity.as_ref().map(|capacity| {
        format!(
            "{} / {}",
            fmt_size(capacity.used_bytes),
            fmt_size(capacity.total_bytes)
        )
    });
    #[cfg(feature = "mount")]
    let mounted = app.mounted.is_some();
    #[cfg(not(feature = "mount"))]
    let mounted = false;
    let unlocked = app.is_unlocked();

    ui.horizontal(|ui| {
        seal(ui, p);
        ui.add_space(2.0);
        let name_width = (ui.available_width() - 64.0).max(40.0);
        ui.vertical(|ui| {
            ui.set_max_width(name_width);
            ui.add_space(1.0);
            ui.add(
                egui::Label::new(RichText::new(name).size(14.5).strong().color(p.text)).truncate(),
            );
            if unlocked && let Some(capacity) = capacity {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if mounted {
                        let (rect, _) = ui.allocate_exact_size(vec2(8.0, 12.0), Sense::hover());
                        ui.painter().circle_filled(rect.center(), 3.0, p.accent);
                    }
                    ui.label(RichText::new(capacity).size(11.0).color(p.muted));
                });
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if flat_icon_button(ui, p, Icon::X, 26.0).clicked() {
                app.dismiss_flyout(ui.ctx());
            }
            if flat_icon_button(ui, p, Icon::ExternalLink, 26.0)
                .on_hover_text(ts!("gui.tray.show"))
                .clicked()
            {
                app.reveal_from_tray(ui.ctx());
                app.dismiss_flyout(ui.ctx());
            }
        });
    });
    ui.add_space(2.0);
    fading_rule(ui, p);
}

/// Short gold rule, fading at both ends. Same mark as the locked page, kept
/// off the window edge so the rounded corners stay clean.
fn fading_rule(ui: &mut Ui, p: Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    let rule = Rect::from_center_size(rect.center(), vec2(rect.width().min(148.0), 1.0));
    let mid = rule.center().x;
    let left = Rect::from_min_max(rule.min, pos2(mid, rule.max.y));
    let right = Rect::from_min_max(pos2(mid, rule.min.y), rule.max);
    let mut mesh = egui::Mesh::default();
    let edge = with_alpha(p.accent, 0.0);
    let core = with_alpha(p.accent, 0.55);
    mesh.colored_vertex(left.left_top(), edge);
    mesh.colored_vertex(left.right_top(), core);
    mesh.colored_vertex(left.right_bottom(), core);
    mesh.colored_vertex(left.left_bottom(), edge);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    mesh.colored_vertex(right.left_top(), core);
    mesh.colored_vertex(right.right_top(), edge);
    mesh.colored_vertex(right.right_bottom(), edge);
    mesh.colored_vertex(right.left_bottom(), core);
    mesh.add_triangle(4, 5, 6);
    mesh.add_triangle(4, 6, 7);
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn header_name(app: &App) -> String {
    if app.is_unlocked() && !app.path_input.is_empty() {
        vault_display_name(&app.path_input)
    } else if !app.flyout.vault_path.is_empty() {
        vault_display_name(&app.flyout.vault_path)
    } else {
        ts!("gui.locked.title")
    }
}

fn seal(ui: &mut Ui, p: Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
    let center = rect.center();
    ui.painter()
        .circle_stroke(center, 14.0, Stroke::new(1.0, with_alpha(p.accent, 0.85)));
    let logo = Rect::from_center_size(center, vec2(18.0, 18.0));
    egui::Image::new(crate::gui::LOGO.clone())
        .fit_to_exact_size(logo.size())
        .paint_at(ui, logo);
}

fn locked(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let rest = ui.available_rect_before_wrap();
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rest)
            .layout(Layout::bottom_up(Align::Min)),
        |ui| {
            if !app.status.is_empty() {
                status_line(ui, p, &app.status);
            }
            let height = ui.available_height().max(80.0);
            ScrollArea::vertical()
                .id_salt("flyout-locked")
                .auto_shrink([false, false])
                .max_height(height)
                .show(ui, |ui| {
                    locked_body(app, ui);
                });
        },
    );
}

fn locked_body(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    ui.label(
        RichText::new(ts!("gui.tray.flyout.locked_hint"))
            .size(12.0)
            .color(p.muted),
    );
    let chosen = !app.flyout.vault_path.is_empty();
    if chosen {
        let response = ui
            .add_enabled_ui(!app.flyout.no_password, |ui| {
                input_field_sized(
                    ui,
                    p,
                    &mut app.flyout.password,
                    ts!("gui.locked.password_hint"),
                    Some(Icon::Key),
                    true,
                    ui.available_width(),
                    36.0,
                    0.95,
                )
            })
            .inner;
        let submit = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if app.flyout.focus_password && !app.flyout.no_password {
            app.flyout.focus_password = false;
            response.request_focus();
        }
        check_box(
            ui,
            p,
            &mut app.flyout.no_password,
            ts!("gui.locked.no_password"),
        );
        let unlock = primary_button(ui, p, Some(Icon::Lock), ts!("gui.locked.open_vault"), 0.95);
        if submit || unlock.clicked() {
            app.unlock_from_flyout();
        }
    } else {
        ui.add_space(4.0);
        if primary_button(ui, p, Some(Icon::Folder), ts!("gui.locked.browse"), 0.95).clicked() {
            choose_vault(app, ui);
        }
    }

    ui.add_space(4.0);
    section_label(ui, p, ts!("gui.locked.recent"));
    let recents: Vec<(String, u64)> = app
        .recent
        .entries
        .iter()
        .take(5)
        .map(|entry| (entry.path.display().to_string(), entry.last_opened))
        .collect();
    if recents.is_empty() {
        ui.label(
            RichText::new(ts!("gui.tray.flyout.no_vault"))
                .size(12.0)
                .color(p.faint),
        );
    } else {
        let current = app.flyout.vault_path.clone();
        for (path, opened) in recents {
            recent_row(app, ui, &path, opened, path == current);
        }
    }
    if chosen
        && chip_button(
            ui,
            p,
            Some(Icon::FolderOpen),
            ts!("gui.tray.flyout.change"),
            p.muted,
        )
        .clicked()
    {
        choose_vault(app, ui);
    }
}

fn choose_vault(app: &mut App, ui: &mut Ui) {
    let path = app.with_native_modal(|| {
        rfd::FileDialog::new()
            .add_filter(&ts!("gui.sesame_filter"), &["sesame"])
            .pick_file()
    });
    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
    if let Some(path) = path {
        app.flyout.vault_path = path.to_string_lossy().into_owned();
        app.flyout.focus_password = true;
    }
}

fn recent_row(app: &mut App, ui: &mut Ui, path: &str, opened: u64, current: bool) {
    let p = app.palette;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let response = clickable(response);
    let hover = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.1);
    let selected = ui
        .ctx()
        .animate_bool_with_time(response.id.with("sel"), current, 0.14);
    if ui.is_rect_visible(rect) {
        let bg = lerp_color(
            with_alpha(p.surface_elevated, hover * 0.85),
            p.accent_soft,
            selected,
        );
        ui.painter().rect_filled(rect, 8.0, bg);
        let icon =
            Rect::from_center_size(pos2(rect.left() + 16.0, rect.center().y), vec2(15.0, 15.0));
        paint_icon(
            ui,
            Icon::FolderLock,
            icon,
            lerp_color(p.muted, p.accent, selected),
        );
        let when = super::widgets::timestamp_label(opened);
        let when_galley = ui
            .painter()
            .layout_no_wrap(when, FontId::proportional(11.0), p.faint);
        ui.painter().galley(
            pos2(
                rect.right() - 8.0 - when_galley.size().x,
                rect.center().y - when_galley.size().y * 0.5,
            ),
            when_galley,
            p.faint,
        );
        let name = vault_display_name(path);
        let galley = truncated_galley(
            ui,
            &name,
            FontId::proportional(13.0),
            p.text,
            rect.width() - 120.0,
        );
        ui.painter().galley(
            pos2(rect.left() + 32.0, rect.center().y - galley.size().y * 0.5),
            galley,
            p.text,
        );
    }
    if response.clicked() {
        app.flyout.vault_path = path.to_owned();
        if app.flyout.no_password {
            app.unlock_from_flyout();
        } else {
            app.flyout.focus_password = true;
        }
    }
}

fn unlocked(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    if let Some(job) = app.current_job() {
        job_line(ui, p, job);
    }
    input_field_sized(
        ui,
        p,
        &mut app.flyout.filter,
        ts!("gui.tray.flyout.search"),
        None,
        false,
        ui.available_width(),
        34.0,
        0.92,
    );

    let rows = visible_rows(app);
    let place = folder_label(&app.flyout.dir);
    if !app.flyout.dir.is_empty()
        && chip_button(ui, p, Some(Icon::FolderOpen), format!("←  {place}"), p.text).clicked()
        && !app.has_job()
    {
        leave_dir(app);
    }

    #[cfg(feature = "mount")]
    let mounted = app.mounted.is_some();
    #[cfg(not(feature = "mount"))]
    let mounted = false;
    let hovering = ui.ctx().input(|input| !input.raw.hovered_files.is_empty());
    let suppress = std::mem::take(&mut app.flyout.suppress_click);
    // Footer is placed from the bottom of whatever space is left, so a taller
    // font cannot push the buttons outside the window clip.
    let rest = ui.available_rect_before_wrap();
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rest)
            .layout(Layout::bottom_up(Align::Min)),
        |ui| {
            footer(app, ui, mounted);
            ui.add_space(2.0);
            fading_rule(ui, p);
            ui.add_space(4.0);
            if !app.status.is_empty() {
                status_line(ui, p, &app.status);
            }
            let list_h = ui.available_height().max(72.0);
            let list = ui
                .allocate_ui(vec2(ui.available_width(), list_h), |ui| {
                    egui::Frame::new()
                        .fill(p.surface)
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(12.0)
                        .inner_margin(egui::Margin::symmetric(6, 8))
                        .show(ui, |ui| {
                            if !rows.is_empty() {
                                hint_row(ui, p, &ts!("gui.tray.flyout.drop_idle"));
                                ui.add_space(2.0);
                            }
                            let scroll_h = ui.available_height().max(48.0);
                            ScrollArea::vertical()
                                .id_salt("flyout-files")
                                .auto_shrink([false, false])
                                .max_height(scroll_h)
                                .show(ui, |ui| {
                                    if rows.is_empty() {
                                        empty_folder(ui, p);
                                        return;
                                    }
                                    for row in &rows {
                                        file_row(app, ui, row, suppress);
                                    }
                                });
                        })
                })
                .inner;
            if hovering && !app.has_job() {
                drop_overlay(ui, p, list.response.rect, &place);
            }
        },
    );

    let typing = ui.memory(|mem| mem.focused().is_some());
    if !typing && app.dialog.is_none() && !app.has_job() {
        keyboard(app, ui, &rows);
    }
}

fn job_line(ui: &mut Ui, p: Palette, job: &super::job::ActiveJob) {
    let (done, total, detail) = job.snapshot();
    let title = if detail.is_empty() {
        job.title.clone()
    } else {
        detail
    };
    ui.add(egui::Label::new(RichText::new(title).size(11.0).color(p.muted)).truncate());
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 3.0), Sense::hover());
    ui.painter().rect_filled(bar, 2.0, p.surface_elevated);
    let fraction = if total == 0 {
        let travel = (ui.input(|input| input.time) as f32 * 0.35).fract();
        let span = bar.width() * 0.35;
        let head = bar.left() + bar.width() * travel;
        ui.painter().rect_filled(
            Rect::from_min_max(
                pos2(head, bar.top()),
                pos2((head + span).min(bar.right()), bar.bottom()),
            ),
            2.0,
            p.accent,
        );
        travel
    } else {
        let fraction = (done as f32 / total as f32).clamp(0.0, 1.0);
        ui.painter().rect_filled(
            Rect::from_min_max(
                bar.min,
                pos2(bar.left() + bar.width() * fraction, bar.bottom()),
            ),
            2.0,
            p.accent,
        );
        fraction
    };
    let _ = fraction;
    ui.ctx().request_repaint_after(Duration::from_millis(80));
}

fn hint_row(ui: &mut Ui, p: Palette, text: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    let icon = Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), vec2(12.0, 12.0));
    paint_icon(ui, Icon::Download, icon, p.faint);
    let galley = truncated_galley(
        ui,
        text,
        FontId::proportional(11.5),
        p.faint,
        rect.width() - 36.0,
    );
    ui.painter().galley(
        pos2(rect.left() + 28.0, rect.center().y - galley.size().y * 0.5),
        galley,
        p.faint,
    );
}

fn empty_folder(ui: &mut Ui, p: Palette) {
    ui.add_space(28.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
        paint_icon(ui, Icon::Folder, rect, with_alpha(p.accent, 0.75));
        ui.add_space(6.0);
        ui.label(
            RichText::new(ts!("gui.browser.empty_folder"))
                .size(13.0)
                .color(p.text),
        );
        ui.label(
            RichText::new(ts!("gui.tray.flyout.drop_idle"))
                .size(12.0)
                .color(p.muted),
        );
    });
}

fn drop_overlay(ui: &Ui, p: Palette, rect: Rect, place: &str) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 10.0, with_alpha(p.bg, 0.88));
    painter.rect_stroke(
        rect.shrink(8.0),
        10.0,
        Stroke::new(1.0, p.accent),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        ts!("gui.browser.drop_here", dir = place),
        FontId::proportional(13.5),
        p.text,
    );
}

fn file_row(app: &mut App, ui: &mut Ui, row: &Row, suppress: bool) {
    let p = app.palette;
    let selected = app.flyout.selected.as_deref() == Some(row.path.as_str());
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let response = clickable(response);
    let hover = ui
        .ctx()
        .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.1);
    let sel = ui
        .ctx()
        .animate_bool_with_time(response.id.with("sel"), selected, 0.14);
    if ui.is_rect_visible(rect) {
        let bg = lerp_color(
            with_alpha(p.surface_elevated, hover * 0.9),
            p.accent_soft,
            sel,
        );
        ui.painter().rect_filled(rect, 8.0, bg);
        if sel > 0.0 {
            let bar = Rect::from_min_size(
                pos2(rect.left() + 2.0, rect.center().y - 8.0 * sel),
                vec2(2.0, 16.0 * sel),
            );
            ui.painter().rect_filled(bar, 1.5, p.accent);
        }
        let (icon, tint) = if row.is_dir {
            (Icon::Folder, p.accent)
        } else {
            (Icon::File, p.muted)
        };
        paint_icon(
            ui,
            icon,
            Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(15.0, 15.0)),
            tint,
        );
        let meta =
            ui.painter()
                .layout_no_wrap(row.meta.clone(), FontId::proportional(11.0), p.faint);
        let meta_x = rect.right() - 10.0 - meta.size().x;
        ui.painter().galley(
            pos2(meta_x, rect.center().y - meta.size().y * 0.5),
            meta,
            p.faint,
        );
        let name_w = (meta_x - rect.left() - 40.0).max(24.0);
        let color = if row.is_dir {
            lerp_color(p.text, p.accent, 0.25 + 0.75 * sel)
        } else {
            p.text
        };
        let galley = truncated_galley(ui, &row.name, FontId::proportional(13.0), color, name_w);
        ui.painter().galley(
            pos2(rect.left() + 34.0, rect.center().y - galley.size().y * 0.5),
            galley,
            color,
        );
    }

    if suppress || app.has_job() {
        return;
    }
    if response.double_clicked() && !row.is_dir {
        app.flyout.selected = Some(row.path.clone());
        app.extract_one(&row.path);
    } else if response.clicked() {
        if row.is_dir {
            enter_dir(app, &row.path);
        } else {
            app.flyout.selected = Some(row.path.clone());
        }
    }
    response.context_menu(|ui| {
        ui.set_min_width(168.0);
        if row.is_dir
            && super::widgets::menu_item(ui, p, Icon::FolderOpen, ts!("gui.browser.open")).clicked()
        {
            enter_dir(app, &row.path);
            ui.close();
        }
        if super::widgets::menu_item(ui, p, Icon::Download, ts!("gui.browser.extract")).clicked() {
            app.flyout.selected = Some(row.path.clone());
            app.extract_one(&row.path);
            ui.close();
        }
    });
}

fn footer(app: &mut App, ui: &mut Ui, mounted: bool) {
    let p = app.palette;
    let busy = app.has_job();
    let can_extract = app.flyout.selected.is_some() && !busy;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let add_tone = if busy { p.faint } else { p.accent };
        if chip_button(
            ui,
            p,
            Some(Icon::Plus),
            ts!("gui.browser.add_files"),
            add_tone,
        )
        .clicked()
            && !busy
        {
            let dir = app.flyout.dir.clone();
            let paths = app.with_native_modal(|| rfd::FileDialog::new().pick_files());
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            if let Some(paths) = paths {
                app.stage_add(dir);
                app.request_add_files(paths);
            }
        }
        let extract_tone = if can_extract { p.text } else { p.faint };
        if chip_button(
            ui,
            p,
            Some(Icon::Download),
            ts!("gui.browser.extract"),
            extract_tone,
        )
        .clicked()
        {
            if let Some(path) = app.flyout.selected.clone() {
                app.extract_one(&path);
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            } else if !busy {
                app.set_status(ts!(
                    "gui.status.error",
                    error = ts!("gui.tray.flyout.select_to_extract")
                ));
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if chip_button(ui, p, Some(Icon::Lock), ts!("gui.browser.lock"), p.muted).clicked() {
                app.lock();
            }
        });
    });
    if mounted
        && chip_button(
            ui,
            p,
            Some(Icon::Mount),
            ts!("gui.browser.open_mount_location"),
            p.muted,
        )
        .clicked()
    {
        app.open_mount_from_tray();
    }
}

fn status_line(ui: &mut Ui, p: Palette, status: &str) {
    let line = status.lines().next().unwrap_or(status);
    let color = if line.starts_with('✓') {
        p.success
    } else if line.starts_with('✗') {
        p.danger
    } else if line.starts_with('⚠') {
        p.warning
    } else {
        p.muted
    };
    ui.add(egui::Label::new(RichText::new(line).size(11.5).color(color)).truncate());
}

fn keyboard(app: &mut App, ui: &mut Ui, rows: &[Row]) {
    let (up, down, enter, back) = ui.input(|input| {
        (
            input.key_pressed(egui::Key::ArrowUp),
            input.key_pressed(egui::Key::ArrowDown),
            input.key_pressed(egui::Key::Enter),
            input.key_pressed(egui::Key::Backspace),
        )
    });
    if back {
        leave_dir(app);
        return;
    }
    if rows.is_empty() {
        return;
    }
    let current = app
        .flyout
        .selected
        .as_ref()
        .and_then(|path| rows.iter().position(|row| &row.path == path));
    if up || down {
        let next = match (current, down) {
            (None, _) => 0,
            (Some(index), true) => (index + 1).min(rows.len() - 1),
            (Some(index), false) => index.saturating_sub(1),
        };
        app.flyout.selected = Some(rows[next].path.clone());
        return;
    }
    if enter {
        let Some(index) = current else {
            return;
        };
        let row = &rows[index];
        if row.is_dir {
            let path = row.path.clone();
            enter_dir(app, &path);
        } else {
            let path = row.path.clone();
            app.extract_one(&path);
        }
    }
}

fn visible_rows(app: &App) -> Vec<Row> {
    let query = app.flyout.filter.trim().to_lowercase();
    let node = app.browser.node_at(&app.flyout.dir);
    let mut children: Vec<&BrowserNode> = node
        .children
        .values()
        .filter(|child| query.is_empty() || child.name.to_lowercase().contains(&query))
        .collect();
    children.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    children
        .into_iter()
        .map(|child| Row {
            name: child.name.clone(),
            path: child.path.clone(),
            is_dir: child.is_dir,
            meta: if child.is_dir {
                let (files, dirs) = child.counts();
                ts!("gui.browser.dir_count", n = files + dirs)
            } else {
                fmt_size(child.size)
            },
        })
        .collect()
}

fn folder_label(dir: &str) -> String {
    if dir.is_empty() {
        ts!("gui.browser.breadcrumb_root")
    } else {
        dir.rsplit('/').next().unwrap_or(dir).to_owned()
    }
}

fn enter_dir(app: &mut App, path: &str) {
    app.flyout.dir = path.to_owned();
    app.flyout.selected = None;
    app.flyout.filter.clear();
    app.flyout.suppress_click = true;
}

fn leave_dir(app: &mut App) {
    if app.flyout.dir.is_empty() {
        return;
    }
    app.flyout.dir = app
        .flyout
        .dir
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_owned())
        .unwrap_or_default();
    app.flyout.selected = None;
    app.flyout.suppress_click = true;
}

/// Place the panel in egui points. `anchor` is in physical pixels.
pub(crate) fn place_flyout(
    anchor: &TrayAnchor,
    pixels_per_point: f32,
    screen: Option<Vec2>,
    size: Vec2,
) -> Pos2 {
    let ppp = if pixels_per_point.is_finite() && pixels_per_point > 0.1 {
        pixels_per_point
    } else {
        1.0
    };
    if let Some(pos) = place_on_icon(anchor.icon, ppp, screen, size) {
        return pos;
    }
    if let Some((x, y)) = anchor.cursor {
        let cursor = pos2(x as f32 / ppp, y as f32 / ppp);
        let mut pos = pos2(
            cursor.x - size.x * 0.5,
            above_or_below(cursor.y, cursor.y, size.y),
        );
        pos = clamp_point(pos, size, cursor, screen);
        return pos;
    }
    if let Some(screen) = screen {
        return pos2(
            (screen.x - size.x - SCREEN_MARGIN).max(SCREEN_MARGIN),
            (screen.y - size.y - SCREEN_MARGIN).max(SCREEN_MARGIN),
        );
    }
    pos2(SCREEN_MARGIN, SCREEN_MARGIN)
}

fn place_on_icon(
    icon: Option<TrayRect>,
    ppp: f32,
    screen: Option<Vec2>,
    size: Vec2,
) -> Option<Pos2> {
    let icon = icon?;
    if icon.width < 1.0 || icon.height < 1.0 {
        return None;
    }
    let left = icon.x as f32 / ppp;
    let top = icon.y as f32 / ppp;
    let width = icon.width as f32 / ppp;
    let height = icon.height as f32 / ppp;
    let right = left + width;
    let bottom = top + height;
    let pos = pos2(right - size.x, above_or_below(top, bottom, size.y));
    let anchor = pos2(left + width * 0.5, top + height * 0.5);
    Some(clamp_point(pos, size, anchor, screen))
}

fn above_or_below(top: f32, bottom: f32, height: f32) -> f32 {
    let above = top - GAP - height;
    if above < SCREEN_MARGIN {
        bottom + GAP
    } else {
        above
    }
}

/// Clamp only when the anchor lies on the primary monitor. A tray on a monitor
/// to the left of the origin has negative coordinates and must stay there.
fn clamp_point(pos: Pos2, size: Vec2, anchor: Pos2, screen: Option<Vec2>) -> Pos2 {
    let Some(screen) = screen else {
        let x = if anchor.x >= 0.0 {
            pos.x.max(SCREEN_MARGIN)
        } else {
            pos.x
        };
        let y = if anchor.y >= 0.0 {
            pos.y.max(SCREEN_MARGIN)
        } else {
            pos.y
        };
        return pos2(x, y);
    };
    let on_primary =
        anchor.x >= 0.0 && anchor.y >= 0.0 && anchor.x <= screen.x && anchor.y <= screen.y;
    if !on_primary {
        return pos;
    }
    let max_x = (screen.x - size.x - SCREEN_MARGIN).max(SCREEN_MARGIN);
    let max_y = (screen.y - size.y - SCREEN_MARGIN).max(SCREEN_MARGIN);
    pos2(
        pos.x.clamp(SCREEN_MARGIN, max_x),
        pos.y.clamp(SCREEN_MARGIN, max_y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn icon_at(x: f64, y: f64, w: f64, h: f64) -> TrayAnchor {
        TrayAnchor {
            icon: Some(TrayRect {
                x,
                y,
                width: w,
                height: h,
            }),
            cursor: None,
        }
    }

    #[test]
    fn panel_sits_above_a_bottom_taskbar() {
        let pos = place_flyout(
            &icon_at(1800.0, 1040.0, 40.0, 40.0),
            1.0,
            Some(vec2(1920.0, 1080.0)),
            vec2(320.0, 460.0),
        );
        assert_eq!(pos, pos2(1520.0, 570.0));
    }

    #[test]
    fn panel_hangs_below_a_top_menu_bar() {
        let pos = place_flyout(
            &icon_at(100.0, 2.0, 24.0, 22.0),
            1.0,
            Some(vec2(1920.0, 1080.0)),
            vec2(320.0, 460.0),
        );
        assert_eq!(pos, pos2(8.0, 34.0));
    }

    #[test]
    fn panel_stays_on_the_right_edge() {
        let pos = place_flyout(
            &icon_at(1900.0, 500.0, 16.0, 16.0),
            1.0,
            Some(vec2(1920.0, 1080.0)),
            vec2(320.0, 460.0),
        );
        assert_eq!(pos, pos2(1592.0, 30.0));
    }

    #[test]
    fn panel_keeps_negative_coordinates_on_a_monitor_to_the_left() {
        let pos = place_flyout(
            &icon_at(-400.0, 500.0, 20.0, 20.0),
            1.0,
            Some(vec2(1920.0, 1080.0)),
            vec2(320.0, 460.0),
        );
        assert_eq!(pos, pos2(-700.0, 30.0));
    }

    #[test]
    fn cursor_is_used_when_the_icon_rect_is_missing() {
        let anchor = TrayAnchor {
            icon: None,
            cursor: Some((500.0, 800.0)),
        };
        let pos = place_flyout(&anchor, 1.0, Some(vec2(1920.0, 1080.0)), vec2(320.0, 460.0));
        assert_eq!(pos, pos2(340.0, 330.0));
    }

    #[test]
    fn unknown_anchor_uses_the_bottom_corner() {
        let pos = place_flyout(
            &TrayAnchor::default(),
            1.0,
            Some(vec2(1920.0, 1080.0)),
            vec2(320.0, 460.0),
        );
        assert_eq!(pos, pos2(1592.0, 612.0));
    }

    #[test]
    fn physical_pixels_are_divided_by_the_scale() {
        let pos = place_flyout(
            &icon_at(400.0, 1600.0, 32.0, 32.0),
            2.0,
            None,
            vec2(320.0, 460.0),
        );
        assert_eq!(pos, pos2(8.0, 330.0));
    }

    #[test]
    fn blur_dismisses_immediately_after_focus() {
        let mut flyout = FlyoutState::default();
        flyout.open = true;
        assert!(!flyout.should_dismiss(Some(false), false));
        assert!(!flyout.should_dismiss(Some(true), false));
        assert!(flyout.should_dismiss(Some(false), false));
        assert!(!flyout.should_dismiss(Some(false), true));
    }

    #[test]
    fn tray_click_that_blurred_the_panel_does_not_reopen_it() {
        let mut flyout = FlyoutState::default();
        flyout.open = true;
        flyout.note_blur_close();
        flyout.shut();
        assert!(flyout.swallows_tray_reopen());
        flyout.open_with(TrayAnchor::default());
        assert!(!flyout.swallows_tray_reopen());
    }
}
