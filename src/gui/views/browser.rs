//! Unlocked vault browser: sidebar (vault card, actions, mount) + file list
//! with search, sortable columns, inline rename, keyboard navigation and a
//! drag-and-drop overlay.

use super::Event;
use crate::app::fmt_size;
use crate::gui::app::{App, FocusTarget, SortKey};
use crate::gui::browser::BrowserNode;
use crate::gui::icons::{Icon, paint_icon};
use crate::gui::layout::fill_background;
use crate::gui::theme::{Palette, ease_out, lerp_color, with_alpha};
use crate::gui::widgets::{
    badge, capacity_bar, chip_button, clickable, flat_icon_button, ghost_button, hairline,
    input_field_sized, menu_item, menu_item_colored, nav_button, paint_chevron, paint_sort_arrow,
    section_label, show_toast, status_dot, timestamp_label, vault_display_name,
};
use egui::{
    Align, Color32, FontId, Frame, Layout, Rect, RichText, ScrollArea, Sense, Stroke, StrokeKind,
    Ui, pos2, vec2,
};
use std::time::Duration;

const ROW_H: f32 = 38.0;
const COL_SIZE_W: f32 = 84.0;
const COL_MODIFIED_W: f32 = 112.0;

pub(crate) fn show(app: &mut App, ui: &mut egui::Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    // Sampled before any widget runs: a text field that consumes Enter/Esc
    // drops focus during this frame, and the list shortcuts must not see the
    // same key press afterwards.
    let field_had_focus = ui.memory(|mem| mem.focused().is_some());
    let dropped = ui.ctx().input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .filter_map(|file| file.path.clone())
            .collect::<Vec<_>>()
    });
    if !dropped.is_empty() {
        events.push(Event::AddPaths(dropped));
    }

    // Fade the whole page in right after unlocking.
    let fade = app
        .unlocked_at
        .map(|at| ease_out(at.elapsed().as_secs_f32() / 0.4))
        .unwrap_or(1.0);
    if fade < 1.0 {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }

    fill_background(ui, p.bg);
    ui.set_opacity(fade);
    sidebar(app, ui, events);

    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.bg).inner_margin(egui::Margin {
            left: 22,
            right: 22,
            top: 16,
            bottom: 16,
        }))
        .show_inside(ui, |ui| {
            // Slide-up for the content column.
            let offset = (1.0 - fade) * 10.0;
            ui.add_space(offset);
            header(app, ui, events);
            ui.add_space(12.0);
            toolbar(app, ui, events);
            ui.add_space(10.0);
            file_list(app, ui, events);
            notification_panel(app, ui, events);
        });
    if !field_had_focus {
        keyboard(app, ui, events);
    }
    show_toast(ui, app);
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

fn sidebar(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    egui::Panel::left("vault_sidebar")
        .resizable(false)
        .exact_size(p.sidebar_width())
        .frame(Frame::new().fill(p.bg_raised).inner_margin(egui::Margin {
            left: 16,
            right: 16,
            top: 14,
            bottom: 14,
        }))
        .show_inside(ui, |ui| {
            // Right edge hairline.
            let edge = ui.max_rect();
            ui.painter().line_segment(
                [
                    pos2(edge.right() + 16.0, edge.top() - 18.0),
                    pos2(edge.right() + 16.0, edge.bottom() + 16.0),
                ],
                Stroke::new(1.0, p.border),
            );
            // Footer (settings + lock) is pinned; everything above scrolls when
            // the window is short.
            const FOOTER_H: f32 = 36.0 + 6.0 + 1.0 + 8.0 + 10.0;
            let body_h = (ui.available_height() - FOOTER_H).max(0.0);
            ScrollArea::vertical()
                .id_salt("sidebar_scroll")
                .max_height(body_h)
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    vault_card(app, ui);
                    ui.add_space(10.0);
                    section_label(ui, p, crate::ts!("gui.browser.section_actions"));
                    ui.add_space(2.0);
                    let width = ui.available_width();
                    if nav_button(
                        ui,
                        p,
                        Some(Icon::Plus),
                        crate::ts!("gui.browser.add_files"),
                        width,
                        false,
                    )
                    .clicked()
                    {
                        events.push(Event::AddFiles);
                    }
                    if nav_button(
                        ui,
                        p,
                        Some(Icon::FolderOpen),
                        crate::ts!("gui.browser.add_folder"),
                        width,
                        false,
                    )
                    .clicked()
                    {
                        events.push(Event::AddFolder);
                    }
                    if nav_button(
                        ui,
                        p,
                        Some(Icon::Refresh),
                        crate::ts!("gui.browser.refresh"),
                        width,
                        false,
                    )
                    .clicked()
                    {
                        events.push(Event::Refresh);
                    }

                    ui.add_space(8.0);
                    #[cfg(feature = "mount")]
                    mount_section(app, ui, events);
                    #[cfg(not(feature = "mount"))]
                    {
                        section_label(ui, p, crate::ts!("settings.mounting"));
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(crate::ts!("gui.browser.mount_not_compiled"))
                                .size(11.0)
                                .color(p.muted),
                        );
                    }
                    ui.add_space(4.0);
                });

            ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let pair = (ui.available_width() - 6.0) * 0.5;
                    if nav_button(
                        ui,
                        p,
                        Some(Icon::Settings),
                        crate::ts!("gui.browser.settings"),
                        pair,
                        false,
                    )
                    .clicked()
                    {
                        events.push(Event::ShowSettings);
                    }
                    if lock_button(ui, p, pair).clicked() {
                        events.push(Event::Lock);
                    }
                });
                ui.add_space(6.0);
                hairline(ui, p);
                ui.add_space(8.0);
            });
        });
}

/// Lock button: neutral at rest, warms to danger on hover.
fn lock_button(ui: &mut Ui, p: Palette, width: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 36.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover =
            ui.ctx()
                .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
        ui.painter().rect_filled(
            rect,
            8.0,
            lerp_color(Color32::TRANSPARENT, p.danger_soft, hover),
        );
        let fg = lerp_color(p.text, p.danger, hover);
        let icon_fg = lerp_color(p.muted, p.danger, hover);
        let x = rect.left() + 12.0;
        paint_icon(
            ui,
            Icon::Lock,
            Rect::from_center_size(pos2(x + 8.0, rect.center().y), vec2(16.0, 16.0)),
            icon_fg,
        );
        let galley = ui.painter().layout_no_wrap(
            crate::ts!("gui.browser.lock"),
            FontId::proportional(13.0),
            fg,
        );
        ui.painter().galley(
            pos2(x + 26.0, rect.center().y - galley.size().y * 0.5),
            galley,
            fg,
        );
    }
    clickable(response)
}

fn vault_card(app: &App, ui: &mut Ui) {
    let p = app.palette;
    Frame::new()
        .fill(p.surface)
        .stroke((1.0, p.border))
        .corner_radius(12.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (icon_rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
                ui.painter().rect_filled(icon_rect, 10.0, p.accent_soft);
                paint_icon(ui, Icon::FolderLock, icon_rect.shrink(9.0), p.accent);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        RichText::new(crate::ts!("gui.browser.sidebar_title"))
                            .size(10.5)
                            .color(p.muted),
                    );
                    let name = vault_display_name(&app.path_input);
                    ui.add(
                        egui::Label::new(RichText::new(name).size(14.0).strong().color(p.text))
                            .truncate(),
                    )
                    .on_hover_text(&app.path_input);
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let (dot_rect, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                status_dot(ui, dot_rect.center(), p.success, true);
                ui.add_space(2.0);
                badge(ui, crate::ts!("gui.browser.unlocked"), p.success);
                if let Some(capacity) = app.capacity {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let used = fmt_size(capacity.used_bytes);
                        let total = fmt_size(capacity.total_bytes);
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("{:.0}%", capacity.ratio() * 100.0))
                                    .size(11.0)
                                    .color(p.muted),
                            )
                            .selectable(false),
                        )
                        .on_hover_text(crate::ts!(
                            "gui.browser.capacity",
                            used = used,
                            total = total
                        ));
                    });
                }
            });
            if let Some(capacity) = app.capacity {
                ui.add_space(8.0);
                capacity_bar(ui, p, capacity.ratio());
            }
        });
}

#[cfg(feature = "mount")]
fn mount_section(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    section_label(ui, p, crate::ts!("settings.mounting"));
    ui.add_space(4.0);
    let mounted = app.mounted.is_some();
    Frame::new()
        .fill(p.surface)
        .stroke((1.0, p.border))
        .corner_radius(12.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (dot_rect, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                status_dot(
                    ui,
                    dot_rect.center(),
                    if mounted { p.success } else { p.faint },
                    mounted,
                );
                ui.add_space(2.0);
                let text = if let Some(mounted) = app.mounted.as_ref() {
                    crate::ts!(
                        "gui.browser.mounted_at",
                        point = mounted.mount_point().display().to_string()
                    )
                } else {
                    crate::ts!("gui.browser.not_mounted")
                };
                ui.add(
                    egui::Label::new(RichText::new(text).size(12.0).color(if mounted {
                        p.text
                    } else {
                        p.muted
                    }))
                    .truncate(),
                );
            });
            ui.add_space(8.0);
            if !mounted {
                input_field_sized(
                    ui,
                    p,
                    &mut app.mount_point_input,
                    crate::ts!("gui.browser.mount_hint"),
                    Some(Icon::HardDrive),
                    false,
                    ui.available_width(),
                    34.0,
                    0.92,
                );
                ui.add_space(8.0);
                if crate::gui::widgets::primary_button_sized(
                    ui,
                    p,
                    Some(Icon::Mount),
                    crate::ts!("gui.browser.mount"),
                    ui.available_width(),
                    34.0,
                    0.92,
                )
                .clicked()
                {
                    events.push(Event::Mount);
                }
            } else {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let half = (ui.available_width() - 6.0) * 0.5;
                    if ghost_pair(
                        ui,
                        p,
                        Some(Icon::ExternalLink),
                        crate::ts!("gui.browser.open_mount"),
                        half,
                    )
                    .on_hover_text(crate::ts!("gui.browser.open_mount_location"))
                    .clicked()
                    {
                        events.push(Event::OpenMountLocation);
                    }
                    if ghost_pair(
                        ui,
                        p,
                        Some(Icon::X),
                        crate::ts!("gui.browser.unmount"),
                        half,
                    )
                    .clicked()
                    {
                        events.push(Event::Unmount);
                    }
                });
            }
            ui.add_space(8.0);
            let mut create_symlinks = app.create_symlinks;
            let inner = ui.add_enabled_ui(mounted, |ui| {
                crate::gui::widgets::toggle_switch(
                    ui,
                    p,
                    &mut create_symlinks,
                    crate::ts!("gui.browser.symlink_on_add"),
                )
            });
            if inner.inner {
                events.push(Event::SetCreateSymlinks(create_symlinks));
            }
            if !mounted {
                let hint = crate::ts!("gui.browser.symlink_requires_mount");
                inner
                    .response
                    .on_hover_text(hint.clone())
                    .on_disabled_hover_text(hint);
            }
        });
}

/// Fixed-width ghost button used for side-by-side pairs.
#[cfg(feature = "mount")]
fn ghost_pair(
    ui: &mut Ui,
    p: Palette,
    icon: Option<Icon>,
    label: String,
    width: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover =
            ui.ctx()
                .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
        ui.painter()
            .rect_filled(rect, 8.0, with_alpha(p.surface_elevated, hover));
        ui.painter().rect_stroke(
            rect,
            8.0,
            (1.0, lerp_color(p.border, p.accent, hover)),
            StrokeKind::Inside,
        );
        let color = lerp_color(p.muted, p.text, hover);
        let galley = ui
            .painter()
            .layout(label, FontId::proportional(11.5), color, width - 36.0);
        let icon_w = if icon.is_some() { 12.0 + 6.0 } else { 0.0 };
        let total = icon_w + galley.size().x;
        let mut x = rect.center().x - total * 0.5;
        if let Some(icon) = icon {
            paint_icon(
                ui,
                icon,
                Rect::from_center_size(pos2(x + 6.0, rect.center().y), vec2(12.0, 12.0)),
                color,
            );
            x += icon_w;
        }
        ui.painter().galley(
            pos2(x, rect.center().y - galley.size().y * 0.5),
            galley,
            color,
        );
    }
    clickable(response)
}

// ---------------------------------------------------------------------------
// Header: breadcrumbs ? search ? bell
// ---------------------------------------------------------------------------

fn header(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>) {
    ui.horizontal(|ui| {
        ui.set_height(36.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            notification_bell(app, ui, events);
            ui.add_space(4.0);
            search_field(app, ui);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                breadcrumbs(app, ui, events);
            });
        });
    });
}

fn search_field(app: &mut App, ui: &mut Ui) {
    let p = app.palette;
    let width = (ui.available_width() * 0.4).clamp(150.0, 240.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 34.0), Sense::hover());
    let id = ui.id().with("search_edit");
    if app.focus_request == Some(FocusTarget::Search) {
        ui.memory_mut(|mem| mem.request_focus(id));
        app.focus_request = None;
    }
    let has_focus = ui.memory(|mem| mem.has_focus(id));
    let focus = ui
        .ctx()
        .animate_bool_with_time(id.with("focus"), has_focus, 0.15);
    let active = has_focus || !app.filter.is_empty();
    ui.painter().rect_filled(rect, 9.0, p.surface);
    ui.painter().rect_stroke(
        rect,
        9.0,
        (1.0, lerp_color(p.border, p.accent, focus)),
        StrokeKind::Inside,
    );
    paint_search_glyph(
        ui.painter(),
        pos2(rect.left() + 16.0, rect.center().y),
        if active { p.accent } else { p.muted },
    );
    let text_rect = Rect::from_min_max(
        pos2(rect.left() + 30.0, rect.top()),
        pos2(rect.right() - 28.0, rect.bottom()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let response = child.add(
        egui::TextEdit::singleline(&mut app.filter)
            .id(id)
            .frame(egui::Frame::NONE)
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::ZERO)
            .font(FontId::proportional(12.5))
            .text_color(p.text)
            .hint_text(
                RichText::new(crate::ts!("gui.browser.search_hint"))
                    .color(p.faint)
                    .size(12.5),
            ),
    );
    if response.has_focus() && child.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.filter.clear();
        response.surrender_focus();
    }
    if !app.filter.is_empty() {
        let clear_rect =
            Rect::from_center_size(pos2(rect.right() - 16.0, rect.center().y), vec2(20.0, 20.0));
        let response = clickable(ui.interact(clear_rect, id.with("clear"), Sense::click()));
        paint_icon(
            ui,
            Icon::X,
            clear_rect.shrink(4.0),
            if response.hovered() { p.text } else { p.muted },
        );
        if response.clicked() {
            app.filter.clear();
        }
    }
}

fn paint_search_glyph(painter: &egui::Painter, center: egui::Pos2, color: Color32) {
    let stroke = Stroke::new(1.6, color);
    painter.circle_stroke(center + vec2(-1.5, -1.5), 5.0, stroke);
    painter.line_segment([center + vec2(2.2, 2.2), center + vec2(6.0, 6.0)], stroke);
}

/// Bell button with an unread badge; toggles the notification history panel.
fn notification_bell(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    let (rect, response) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
    let response = clickable(response);
    let hover = ui.ctx().animate_bool_with_time(
        response.id.with("hover"),
        response.hovered() || app.notifications_open,
        0.12,
    );
    ui.painter()
        .rect_filled(rect, 9.0, with_alpha(p.surface_elevated, hover));
    let color = if app.notifications_open {
        p.accent
    } else {
        lerp_color(p.muted, p.text, hover)
    };
    crate::gui::widgets::paint_bell(ui.painter(), rect.center(), 17.0, color);
    let unread = app.unread_count();
    if unread > 0 {
        let badge_center = rect.right_top() + vec2(-8.0, 8.0);
        ui.painter().circle_filled(badge_center, 8.0, p.bg);
        ui.painter().circle_filled(badge_center, 6.5, p.danger);
        let count = if unread > 9 {
            "9+".to_owned()
        } else {
            unread.to_string()
        };
        ui.painter().text(
            badge_center,
            egui::Align2::CENTER_CENTER,
            count,
            FontId::proportional(8.5),
            Color32::WHITE,
        );
    }
    if response.clicked() {
        events.push(Event::ToggleNotifications);
    }
}

fn breadcrumbs(app: &App, ui: &mut Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    ui.spacing_mut().item_spacing.x = 2.0;
    let root_label = crate::ts!("gui.browser.breadcrumb_root");
    if crumb(
        ui,
        p,
        Some(Icon::FolderLock),
        &root_label,
        app.current_dir.is_empty(),
    ) {
        events.push(Event::EnterDirectory(String::new()));
    }
    if app.current_dir.is_empty() {
        return;
    }
    let mut accumulated = String::new();
    let segments: Vec<&str> = app.current_dir.split('/').collect();
    for (index, segment) in segments.iter().enumerate() {
        let (sep, _) = ui.allocate_exact_size(vec2(16.0, 24.0), Sense::hover());
        paint_chevron(ui.painter(), sep.center(), 9.0, p.faint);
        if index > 0 {
            accumulated.push('/');
        }
        accumulated.push_str(segment);
        let is_last = index + 1 == segments.len();
        if crumb(ui, p, None, segment, is_last) {
            events.push(Event::EnterDirectory(accumulated.clone()));
        }
    }
}

fn crumb(ui: &mut Ui, p: Palette, icon: Option<Icon>, label: &str, current: bool) -> bool {
    let font = FontId::proportional(if current { 15.0 } else { 13.0 });
    let color = if current { p.text } else { p.muted };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, color);
    let icon_w = if icon.is_some() { 14.0 + 6.0 } else { 0.0 };
    let size = vec2(galley.size().x + icon_w + 14.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if current {
            Sense::hover()
        } else {
            Sense::click()
        },
    );
    let hover = if current {
        0.0
    } else {
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12)
    };
    if hover > 0.0 {
        ui.painter()
            .rect_filled(rect, 7.0, with_alpha(p.surface_elevated, hover));
    }
    let color = lerp_color(color, p.text, hover);
    let mut x = rect.left() + 7.0;
    if let Some(icon) = icon {
        paint_icon(
            ui,
            icon,
            Rect::from_center_size(pos2(x + 7.0, rect.center().y), vec2(14.0, 14.0)),
            if current { p.accent } else { color },
        );
        x += icon_w;
    }
    ui.painter().galley(
        pos2(x, rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
    if current {
        false
    } else {
        clickable(response).clicked()
    }
}

// ---------------------------------------------------------------------------
// Toolbar: summary ? new folder ? selection actions
// ---------------------------------------------------------------------------

fn toolbar(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    let node = app.browser.node_at(&app.current_dir);
    let (files, dirs) = node.counts();
    let total = node.total_size();
    let summary = crate::ts!(
        "gui.browser.items_summary",
        files = files,
        dirs = dirs,
        size = fmt_size(total)
    );
    ui.horizontal(|ui| {
        ui.set_height(30.0);
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(summary).size(12.0).color(p.muted));
        ui.add_space(6.0);
        if app.selected_paths.is_empty() {
            if chip_button(
                ui,
                p,
                Some(Icon::Plus),
                crate::ts!("gui.browser.new_folder"),
                p.accent,
            )
            .clicked()
            {
                app.mkdir_open = !app.mkdir_open;
                if app.mkdir_open {
                    app.focus_request = Some(FocusTarget::Mkdir);
                } else {
                    app.mkdir_input.clear();
                }
            }
        } else {
            selection_actions(app, ui, events);
        }
    });
    if app.mkdir_open && app.selected_paths.is_empty() {
        ui.add_space(6.0);
        mkdir_row(app, ui, events);
    }
}

fn mkdir_row(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let width = (ui.available_width() * 0.5).clamp(180.0, 320.0);
        let (rect, _) = ui.allocate_exact_size(vec2(width, 34.0), Sense::hover());
        let id = ui.id().with("mkdir_edit");
        if app.focus_request == Some(FocusTarget::Mkdir) {
            ui.memory_mut(|mem| mem.request_focus(id));
            app.focus_request = None;
        }
        let has_focus = ui.memory(|mem| mem.has_focus(id));
        ui.painter().rect_filled(rect, 8.0, p.surface_elevated);
        ui.painter().rect_stroke(
            rect,
            8.0,
            (1.0, if has_focus { p.accent } else { p.border }),
            StrokeKind::Inside,
        );
        paint_icon(
            ui,
            Icon::Folder,
            Rect::from_center_size(pos2(rect.left() + 17.0, rect.center().y), vec2(14.0, 14.0)),
            p.accent,
        );
        let text_rect = Rect::from_min_max(
            pos2(rect.left() + 32.0, rect.top()),
            pos2(rect.right() - 10.0, rect.bottom()),
        );
        let mut submit = false;
        let mut cancel = false;
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(text_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        let response = child.add(
            egui::TextEdit::singleline(&mut app.mkdir_input)
                .id(id)
                .frame(egui::Frame::NONE)
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::ZERO)
                .font(FontId::proportional(13.0))
                .text_color(p.text)
                .hint_text(
                    RichText::new(crate::ts!("gui.browser.new_folder"))
                        .color(p.faint)
                        .size(13.0),
                ),
        );
        if response.lost_focus() && child.input(|i| i.key_pressed(egui::Key::Enter)) {
            submit = true;
        }
        if response.has_focus() && child.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if crate::gui::widgets::primary_button_sized(
            ui,
            p,
            None,
            crate::ts!("gui.browser.create"),
            84.0,
            34.0,
            0.92,
        )
        .clicked()
        {
            submit = true;
        }
        if ghost_button(ui, p, crate::ts!("gui.dialog.cancel")).clicked() {
            cancel = true;
        }
        if submit {
            events.push(Event::Mkdir);
        }
        if cancel {
            app.mkdir_open = false;
            app.mkdir_input.clear();
        }
    });
}

fn selection_actions(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    let count = app.selected_paths.len();
    let single = count == 1;
    let single_file = single
        && app.selected_paths.iter().next().is_some_and(|path| {
            app.entries
                .iter()
                .any(|entry| entry.path == *path && !entry.is_dir)
        });
    badge(
        ui,
        crate::ts!("gui.browser.selection_count", count = count),
        p.accent,
    );
    ui.add_space(2.0);
    if single_file
        && chip_button(
            ui,
            p,
            Some(Icon::ExternalLink),
            crate::ts!("gui.browser.open"),
            p.text,
        )
        .clicked()
    {
        events.push(Event::OpenSelected);
    }
    if chip_button(
        ui,
        p,
        Some(Icon::Download),
        crate::ts!("gui.browser.extract"),
        p.text,
    )
    .clicked()
    {
        events.push(Event::ExtractSelected);
    }
    if chip_button(
        ui,
        p,
        Some(Icon::Copy),
        crate::ts!("gui.browser.copy"),
        p.text,
    )
    .clicked()
    {
        events.push(Event::BeginCopy);
    }
    if chip_button(
        ui,
        p,
        Some(Icon::Folder),
        crate::ts!("gui.browser.move"),
        p.text,
    )
    .clicked()
    {
        events.push(Event::BeginMove);
    }
    if chip_button(
        ui,
        p,
        Some(Icon::Upload),
        crate::ts!("gui.browser.move_out"),
        p.text,
    )
    .clicked()
    {
        events.push(Event::BeginMoveOut);
    }
    if single
        && chip_button(
            ui,
            p,
            Some(Icon::Edit),
            crate::ts!("gui.browser.rename"),
            p.text,
        )
        .clicked()
    {
        events.push(Event::BeginRename);
    }
    if chip_button(
        ui,
        p,
        Some(Icon::Trash),
        crate::ts!("gui.browser.delete"),
        p.danger,
    )
    .clicked()
    {
        events.push(Event::RemoveSelected);
    }
    if flat_icon_button(ui, p, Icon::X, 26.0)
        .on_hover_text(crate::ts!("gui.browser.clear_selection"))
        .clicked()
    {
        events.push(Event::ClearSelection);
    }
}

// ---------------------------------------------------------------------------
// File list
// ---------------------------------------------------------------------------

/// Visible children of the current folder: filtered by the search query and
/// sorted (directories first).
fn visible_children(app: &App, node: &BrowserNode) -> Vec<BrowserNode> {
    let query = app.filter.trim().to_lowercase();
    let mut children: Vec<BrowserNode> = if query.is_empty() {
        node.children.values().cloned().collect()
    } else {
        app.browser.search(&query).into_iter().cloned().collect()
    };
    children.sort_by(|a, b| {
        use std::cmp::Ordering;
        match (a.is_dir, b.is_dir) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        let ordering = match app.sort.key {
            SortKey::Name => natural_cmp(&a.name, &b.name),
            SortKey::Size => a
                .total_size()
                .cmp(&b.total_size())
                .then_with(|| natural_cmp(&a.name, &b.name)),
            SortKey::Modified => a
                .modified_at
                .cmp(&b.modified_at)
                .then_with(|| natural_cmp(&a.name, &b.name)),
        };
        if app.sort.ascending {
            ordering
        } else {
            ordering.reverse()
        }
    });
    children
}

/// Case-insensitive comparison that orders embedded numbers numerically
/// (`file2` < `file10`).
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(ca), Some(cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut na = 0u128;
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na = na
                        .saturating_mul(10)
                        .saturating_add(c as u128 - '0' as u128);
                    ai.next();
                }
                let mut nb = 0u128;
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb = nb
                        .saturating_mul(10)
                        .saturating_add(c as u128 - '0' as u128);
                    bi.next();
                }
                if na != nb {
                    return na.cmp(&nb);
                }
            }
            (Some(ca), Some(cb)) => {
                let la = ca.to_lowercase().next().unwrap_or(ca);
                let lb = cb.to_lowercase().next().unwrap_or(cb);
                if la != lb {
                    return la.cmp(&lb);
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Tint for a file by extension category.
fn file_tint(p: Palette, name: &str) -> Color32 {
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "raw" | "heic" | "svg" | "bmp" | "tif"
        | "tiff" => p.info,
        "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" | "zst" => p.warning,
        "key" | "pem" | "kdbx" | "asc" | "gpg" | "p12" | "pfx" | "sesame" => p.accent,
        "mp4" | "mov" | "mkv" | "mp3" | "flac" | "wav" => p.success,
        _ => p.muted,
    }
}

fn file_list(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    let hovering_files = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
    let node = app.browser.node_at(&app.current_dir).clone();
    let visible = visible_children(app, &node);
    let ordered: Vec<String> = visible.iter().map(|child| child.path.clone()).collect();
    let show_modified = ui.available_width() > 560.0;

    let outer = Frame::new()
        .fill(p.surface)
        .stroke((1.0, p.border))
        .corner_radius(p.card_radius())
        .inner_margin(egui::Margin {
            left: 8,
            right: 8,
            top: 6,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.set_min_height(ui.available_height());
            column_header(app, ui, show_modified);
            ui.add_space(2.0);
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if visible.is_empty() {
                        empty_state(app, ui, node.children.is_empty());
                        return;
                    }
                    for child in &visible {
                        file_row(app, ui, child, &ordered, show_modified, events);
                    }
                    ui.add_space(6.0);
                });
        });

    if hovering_files {
        drop_overlay(app, ui, outer.response.rect);
    }
}

fn column_header(app: &mut App, ui: &mut Ui, show_modified: bool) {
    let p = app.palette;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    let mut right = rect.right() - 10.0;
    let size_x = right - COL_SIZE_W;
    header_cell(
        app,
        ui,
        Rect::from_min_max(pos2(size_x, rect.top()), pos2(right, rect.bottom())),
        SortKey::Size,
        &crate::ts!("gui.browser.col_size"),
        Align::RIGHT,
    );
    right = size_x;
    if show_modified {
        let mod_x = right - COL_MODIFIED_W;
        header_cell(
            app,
            ui,
            Rect::from_min_max(pos2(mod_x, rect.top()), pos2(right, rect.bottom())),
            SortKey::Modified,
            &crate::ts!("gui.browser.col_modified"),
            Align::RIGHT,
        );
        right = mod_x;
    }
    header_cell(
        app,
        ui,
        Rect::from_min_max(
            pos2(rect.left() + 10.0, rect.top()),
            pos2(right, rect.bottom()),
        ),
        SortKey::Name,
        &crate::ts!("gui.browser.col_name"),
        Align::LEFT,
    );
    let line = Rect::from_min_size(
        pos2(rect.left(), rect.bottom() - 1.0),
        vec2(rect.width(), 1.0),
    );
    ui.painter().rect_filled(line, 0.0, p.border);
}

fn header_cell(app: &mut App, ui: &mut Ui, rect: Rect, key: SortKey, label: &str, align: Align) {
    let p = app.palette;
    let response = clickable(ui.interact(rect, ui.id().with("col").with(label), Sense::click()));
    let active = app.sort.key == key;
    let hover =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    let color = lerp_color(if active { p.text } else { p.muted }, p.text, hover);
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(11.0), color);
    let arrow_w = if active { 12.0 } else { 0.0 };
    let x = match align {
        Align::RIGHT => rect.right() - galley.size().x - arrow_w,
        _ => rect.left() + 26.0 + 10.0,
    };
    ui.painter().galley(
        pos2(x, rect.center().y - galley.size().y * 0.5),
        galley.clone(),
        color,
    );
    if active {
        paint_sort_arrow(
            ui.painter(),
            pos2(x + galley.size().x + 7.0, rect.center().y),
            app.sort.ascending,
            p.accent,
        );
    }
    if response.clicked() {
        app.sort.toggle(key);
    }
}

fn file_row(
    app: &mut App,
    ui: &mut Ui,
    child: &BrowserNode,
    ordered: &[String],
    show_modified: bool,
    events: &mut Vec<Event>,
) {
    let p = app.palette;
    let selected = app.selected_paths.contains(&child.path);
    let renaming = app.rename_target.as_deref() == Some(child.path.as_str());
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::click());
    let response = clickable(response);
    let hover =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.10);
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
                pos2(rect.left() + 2.0, rect.center().y - 9.0 * sel),
                vec2(2.5, 18.0 * sel),
            );
            ui.painter().rect_filled(bar, 2.0, p.accent);
        }
        let (icon, tint) = if child.is_dir {
            (Icon::Folder, p.accent)
        } else {
            (Icon::File, file_tint(p, &child.name))
        };
        let icon_rect =
            Rect::from_center_size(pos2(rect.left() + 24.0, rect.center().y), vec2(17.0, 17.0));
        paint_icon(ui, icon, icon_rect, tint);

        // Right columns.
        let mut right = rect.right() - 10.0;
        let size_text = if child.is_dir {
            let (files, dirs) = child.counts();
            crate::ts!("gui.browser.dir_count", n = files + dirs)
        } else {
            fmt_size(child.size)
        };
        let size_galley =
            ui.painter()
                .layout_no_wrap(size_text, FontId::proportional(11.5), p.muted);
        ui.painter().galley(
            pos2(
                right - size_galley.size().x,
                rect.center().y - size_galley.size().y * 0.5,
            ),
            size_galley,
            p.muted,
        );
        right -= COL_SIZE_W;
        if show_modified {
            let modified = timestamp_label(child.modified_at);
            let galley = ui
                .painter()
                .layout_no_wrap(modified, FontId::proportional(11.5), p.muted);
            ui.painter().galley(
                pos2(
                    right - galley.size().x,
                    rect.center().y - galley.size().y * 0.5,
                ),
                galley,
                p.muted,
            );
            right -= COL_MODIFIED_W;
        }

        let name_x = rect.left() + 46.0;
        let name_w = (right - name_x - 8.0).max(40.0);
        if renaming {
            rename_editor(
                app,
                ui,
                Rect::from_min_max(
                    pos2(name_x - 6.0, rect.top() + 5.0),
                    pos2(name_x + name_w, rect.bottom() - 5.0),
                ),
                events,
            );
        } else {
            let color = if child.is_dir {
                lerp_color(p.text, p.accent, 0.35 + 0.65 * sel)
            } else {
                lerp_color(p.text, p.accent, sel)
            };
            // Single line with an ellipsis when the name overflows.
            let label = if app.filter.trim().is_empty() {
                child.name.clone()
            } else {
                child.path.clone()
            };
            let galley = crate::gui::widgets::truncated_galley(
                ui,
                &label,
                FontId::proportional(13.5),
                color,
                name_w,
            );
            ui.painter().galley(
                pos2(name_x, rect.center().y - galley.size().y * 0.5),
                galley,
                color,
            );
            if child.is_dir && hover > 0.0 {
                paint_chevron(
                    ui.painter(),
                    pos2(right - 12.0, rect.center().y),
                    9.0,
                    with_alpha(p.muted, hover),
                );
            }
        }
    }

    if renaming {
        return;
    }

    if response.clicked() {
        let (ctrl, shift) = ui.input(|input| (input.modifiers.command, input.modifiers.shift));
        events.push(Event::SelectEntry {
            path: child.path.clone(),
            ctrl,
            shift,
            ordered: ordered.to_vec(),
        });
    }
    if response.double_clicked() {
        if child.is_dir {
            events.push(Event::EnterDirectory(child.path.clone()));
        } else {
            app.selected_paths.clear();
            app.selected_paths.insert(child.path.clone());
            app.selection_anchor = Some(child.path.clone());
            events.push(Event::OpenSelected);
        }
    }

    response.context_menu(|ui| {
        ui.set_min_width(180.0);
        if !app.selected_paths.contains(&child.path) {
            app.selected_paths.clear();
            app.selected_paths.insert(child.path.clone());
            app.selection_anchor = Some(child.path.clone());
        }
        let single = app.selected_paths.len() == 1;
        if single
            && child.is_dir
            && menu_item(ui, p, Icon::FolderOpen, crate::ts!("gui.browser.open")).clicked()
        {
            events.push(Event::EnterDirectory(child.path.clone()));
            ui.close();
        }
        if single
            && !child.is_dir
            && menu_item(ui, p, Icon::ExternalLink, crate::ts!("gui.browser.open")).clicked()
        {
            events.push(Event::OpenSelected);
            ui.close();
        }
        if menu_item(ui, p, Icon::Download, crate::ts!("gui.browser.extract")).clicked() {
            events.push(Event::ExtractSelected);
            ui.close();
        }
        if menu_item(ui, p, Icon::Copy, crate::ts!("gui.browser.copy")).clicked() {
            events.push(Event::BeginCopy);
            ui.close();
        }
        if menu_item(ui, p, Icon::Folder, crate::ts!("gui.browser.move")).clicked() {
            events.push(Event::BeginMove);
            ui.close();
        }
        if menu_item(ui, p, Icon::Upload, crate::ts!("gui.browser.move_out")).clicked() {
            events.push(Event::BeginMoveOut);
            ui.close();
        }
        if single && menu_item(ui, p, Icon::Edit, crate::ts!("gui.browser.rename")).clicked() {
            events.push(Event::BeginRename);
            ui.close();
        }
        ui.add_space(2.0);
        hairline(ui, p);
        ui.add_space(2.0);
        if menu_item_colored(
            ui,
            p,
            Icon::Trash,
            crate::ts!("gui.browser.delete"),
            p.danger,
        )
        .clicked()
        {
            events.push(Event::RemoveSelected);
            ui.close();
        }
    });
}

/// Inline rename editor drawn inside the row; Enter commits, Esc cancels,
/// clicking elsewhere commits too.
fn rename_editor(app: &mut App, ui: &mut Ui, rect: Rect, events: &mut Vec<Event>) {
    let p = app.palette;
    let id = ui.id().with("rename_edit");
    if app.focus_request == Some(FocusTarget::Rename) {
        ui.memory_mut(|mem| mem.request_focus(id));
        app.focus_request = None;
        // Select the stem so typing replaces the name but keeps the extension.
        let stem_len = app
            .rename_input
            .rsplit_once('.')
            .map(|(stem, _)| stem.chars().count())
            .filter(|len| *len > 0)
            .unwrap_or_else(|| app.rename_input.chars().count());
        let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(stem_len),
            )));
        state.store(ui.ctx(), id);
    }
    ui.painter().rect_filled(rect, 6.0, p.surface);
    ui.painter()
        .rect_stroke(rect, 6.0, (1.0, p.accent), StrokeKind::Inside);
    let text_rect = rect.shrink2(vec2(8.0, 0.0));
    let mut commit = false;
    let mut cancel = false;
    // `new_child` paints into the row without allocating extra space in the
    // list, so the rows below don't shift while editing.
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    let response = child.add(
        egui::TextEdit::singleline(&mut app.rename_input)
            .id(id)
            .frame(egui::Frame::NONE)
            .desired_width(f32::INFINITY)
            .margin(egui::Margin::ZERO)
            .font(FontId::proportional(13.5))
            .text_color(p.text),
    );
    if response.has_focus() && child.input(|i| i.key_pressed(egui::Key::Escape)) {
        cancel = true;
    } else if response.lost_focus() {
        // Enter or click-away both commit; Esc was handled above.
        commit = true;
    }
    if cancel {
        events.push(Event::CancelRename);
    } else if commit {
        events.push(Event::CommitRename);
    }
}

fn empty_state(app: &App, ui: &mut Ui, folder_empty: bool) {
    let p = app.palette;
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.22).clamp(24.0, 110.0));
        if folder_empty {
            // Dashed drop target.
            let (zone, _) = ui
                .allocate_exact_size(vec2(ui.available_width().min(360.0), 150.0), Sense::hover());
            paint_dashed_rect(ui.painter(), zone, 12.0, with_alpha(p.border_strong, 0.9));
            let icon_rect =
                Rect::from_center_size(pos2(zone.center().x, zone.top() + 46.0), vec2(40.0, 40.0));
            paint_icon(ui, Icon::Upload, icon_rect, with_alpha(p.accent, 0.8));
            ui.painter().text(
                pos2(zone.center().x, zone.top() + 84.0),
                egui::Align2::CENTER_CENTER,
                crate::ts!("gui.browser.empty_folder"),
                FontId::proportional(14.5),
                p.text,
            );
            ui.painter().text(
                pos2(zone.center().x, zone.top() + 108.0),
                egui::Align2::CENTER_CENTER,
                crate::ts!("gui.browser.empty_folder_hint"),
                FontId::proportional(12.0),
                p.muted,
            );
            ui.add_space(12.0);
            ui.label(
                RichText::new(crate::ts!("gui.browser.drop_hint"))
                    .size(11.5)
                    .color(p.faint),
            );
        } else {
            let (glyph, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
            paint_search_glyph(ui.painter(), glyph.center(), with_alpha(p.muted, 0.6));
            ui.add_space(6.0);
            ui.label(
                RichText::new(crate::ts!(
                    "gui.browser.no_matches",
                    query = app.filter.trim()
                ))
                .size(14.0)
                .color(p.text),
            );
        }
    });
}

fn paint_dashed_rect(painter: &egui::Painter, rect: Rect, radius: f32, color: Color32) {
    // Rounded-rectangle outline as a polyline (8 segments per corner), then
    // dashed.
    let r = radius.min(rect.width() * 0.5).min(rect.height() * 0.5);
    let corners = [
        (
            pos2(rect.right() - r, rect.top() + r),
            -std::f32::consts::FRAC_PI_2,
        ),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (
            pos2(rect.left() + r, rect.bottom() - r),
            std::f32::consts::FRAC_PI_2,
        ),
        (pos2(rect.left() + r, rect.top() + r), std::f32::consts::PI),
    ];
    let mut points = Vec::with_capacity(4 * 9 + 1);
    for (center, start) in corners {
        for step in 0..=8 {
            let angle = start + step as f32 / 8.0 * std::f32::consts::FRAC_PI_2;
            points.push(pos2(center.x + r * angle.cos(), center.y + r * angle.sin()));
        }
    }
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    painter.add(egui::Shape::dashed_line(
        &points,
        Stroke::new(1.5, color),
        6.0,
        5.0,
    ));
}

/// Full-list overlay while files are dragged over the window.
fn drop_overlay(app: &App, ui: &mut Ui, rect: Rect) {
    let p = app.palette;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, p.card_radius(), with_alpha(p.bg, 0.72));
    paint_dashed_rect(&painter, rect.shrink(10.0), 12.0, p.accent);
    let center = rect.center();
    paint_icon(
        ui,
        Icon::Download,
        Rect::from_center_size(center - vec2(0.0, 30.0), vec2(40.0, 40.0)),
        p.accent,
    );
    let dir = if app.current_dir.is_empty() {
        crate::ts!("gui.browser.breadcrumb_root")
    } else {
        app.current_dir.clone()
    };
    painter.text(
        center + vec2(0.0, 10.0),
        egui::Align2::CENTER_CENTER,
        crate::ts!("gui.browser.drop_here", dir = dir),
        FontId::proportional(16.0),
        p.text,
    );
    ui.ctx().request_repaint();
}

// ---------------------------------------------------------------------------
// Keyboard
// ---------------------------------------------------------------------------

fn keyboard(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>) {
    // Never steal keys from a focused text field or an open dialog.
    if app.dialog.is_some() || ui.memory(|mem| mem.focused().is_some()) {
        return;
    }
    let node = app.browser.node_at(&app.current_dir).clone();
    let ordered: Vec<String> = visible_children(app, &node)
        .iter()
        .map(|child| child.path.clone())
        .collect();
    let (down, up, enter, backspace, delete, f2, select_all, escape, find, home, end) = ui
        .input_mut(|input| {
            let cmd = egui::Modifiers::COMMAND;
            (
                input.key_pressed(egui::Key::ArrowDown),
                input.key_pressed(egui::Key::ArrowUp),
                input.key_pressed(egui::Key::Enter),
                input.key_pressed(egui::Key::Backspace),
                input.key_pressed(egui::Key::Delete),
                input.key_pressed(egui::Key::F2),
                input.consume_key(cmd, egui::Key::A),
                input.key_pressed(egui::Key::Escape),
                input.consume_key(cmd, egui::Key::F),
                input.key_pressed(egui::Key::Home),
                input.key_pressed(egui::Key::End),
            )
        });
    let shift = ui.input(|i| i.modifiers.shift);
    let anchor_index = app
        .selection_anchor
        .as_ref()
        .and_then(|anchor| ordered.iter().position(|path| path == anchor));
    let move_to = |index: usize, events: &mut Vec<Event>| {
        if let Some(path) = ordered.get(index) {
            events.push(Event::SelectEntry {
                path: path.clone(),
                ctrl: false,
                shift,
                ordered: ordered.clone(),
            });
        }
    };
    if !ordered.is_empty() {
        if down {
            let next = anchor_index
                .map(|i| (i + 1).min(ordered.len() - 1))
                .unwrap_or(0);
            move_to(next, events);
        } else if up {
            let next = anchor_index.map(|i| i.saturating_sub(1)).unwrap_or(0);
            move_to(next, events);
        } else if home {
            move_to(0, events);
        } else if end {
            move_to(ordered.len() - 1, events);
        }
    }
    if enter && app.selected_paths.len() == 1 {
        let path = app
            .selected_paths
            .iter()
            .next()
            .cloned()
            .unwrap_or_default();
        let is_dir = app
            .entries
            .iter()
            .any(|entry| entry.path == path && entry.is_dir);
        if is_dir {
            events.push(Event::EnterDirectory(path));
        } else {
            events.push(Event::OpenSelected);
        }
    }
    if backspace {
        events.push(Event::GoUp);
    }
    if delete && !app.selected_paths.is_empty() {
        events.push(Event::RemoveSelected);
    }
    if f2 && app.selected_paths.len() == 1 {
        events.push(Event::BeginRename);
    }
    if select_all {
        events.push(Event::SelectAll);
    }
    if find {
        app.focus_request = Some(FocusTarget::Search);
    }
    if escape {
        if app.notifications_open {
            events.push(Event::ToggleNotifications);
        } else if !app.filter.is_empty() {
            app.filter.clear();
        } else if app.mkdir_open {
            app.mkdir_open = false;
            app.mkdir_input.clear();
        } else if !app.selected_paths.is_empty() {
            events.push(Event::ClearSelection);
        }
    }
}

// ---------------------------------------------------------------------------
// Notification history panel
// ---------------------------------------------------------------------------

pub(crate) fn notification_panel(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    let open =
        ui.ctx()
            .animate_bool_with_time(egui::Id::new("notif_open"), app.notifications_open, 0.16);
    if open <= 0.0 {
        return;
    }
    let anchor = ui.max_rect().right_top() + vec2(0.0, 44.0 - (1.0 - open) * 8.0);
    egui::Area::new(egui::Id::new("notification_panel"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(anchor)
        .interactable(app.notifications_open)
        .show(ui.ctx(), |ui| {
            ui.set_opacity(open);
            Frame::new()
                .fill(p.surface)
                .stroke((1.0, p.border_strong))
                .corner_radius(12.0)
                .inner_margin(egui::Margin::same(14))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 12],
                    blur: 28,
                    spread: 0,
                    color: p.shadow,
                })
                .show(ui, |ui| {
                    ui.set_width(340.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(crate::ts!("gui.notify.title"))
                                .size(14.0)
                                .strong()
                                .color(p.text),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if !app.notifications.is_empty()
                                && ghost_button(ui, p, crate::ts!("gui.notify.clear")).clicked()
                            {
                                events.push(Event::ClearNotifications);
                            }
                        });
                    });
                    ui.add_space(8.0);
                    if app.notifications.is_empty() {
                        ui.add_space(12.0);
                        ui.vertical_centered(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                            crate::gui::widgets::paint_bell(
                                ui.painter(),
                                rect.center(),
                                26.0,
                                with_alpha(p.faint, 0.6),
                            );
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(crate::ts!("gui.notify.empty"))
                                    .size(12.0)
                                    .color(p.muted),
                            );
                        });
                        ui.add_space(12.0);
                        return;
                    }
                    egui::ScrollArea::vertical()
                        .max_height(300.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            let count = app.notifications.len();
                            for (index, note) in app.notifications.iter().rev().enumerate() {
                                notification_row(p, ui, note);
                                if index + 1 < count {
                                    ui.add_space(4.0);
                                    hairline(ui, p);
                                    ui.add_space(4.0);
                                }
                            }
                        });
                });
        });
}

fn notification_row(p: Palette, ui: &mut Ui, note: &crate::gui::app::Notification) {
    ui.horizontal_top(|ui| {
        let color = crate::gui::widgets::kind_color(p, note.kind);
        let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
        crate::gui::widgets::paint_kind_glyph(ui.painter(), rect.center(), note.kind, color);
        ui.add_space(2.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.set_width(ui.available_width());
            let text = crate::gui::widgets::strip_status_glyph(&note.text);
            let mut lines = text.lines();
            if let Some(first) = lines.next() {
                ui.add(egui::Label::new(RichText::new(first).size(12.5).color(p.text)).wrap());
            }
            for line in lines.take(3) {
                ui.add(egui::Label::new(RichText::new(line).size(11.0).color(p.muted)).wrap());
            }
            ui.label(
                RichText::new(crate::gui::widgets::relative_time(
                    note.at.elapsed().as_secs(),
                ))
                .size(10.5)
                .color(p.faint),
            );
        });
    });
}

#[cfg(test)]
mod tests {
    use super::natural_cmp;
    use std::cmp::Ordering;

    #[test]
    fn natural_order_handles_numbers_and_case() {
        assert_eq!(natural_cmp("file2", "file10"), Ordering::Less);
        assert_eq!(natural_cmp("File10", "file2"), Ordering::Greater);
        assert_eq!(natural_cmp("abc", "ABC"), Ordering::Equal);
        assert_eq!(natural_cmp("a", "ab"), Ordering::Less);
    }
}
