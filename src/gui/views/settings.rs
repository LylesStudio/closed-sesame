//! Settings page: themed cards with icon headers, theme swatches, language
//! segments and animated toggles.

use super::Event;
use crate::gui::app::App;
use crate::gui::icons::{Icon, paint_icon};
use crate::gui::layout::fill_background;
use crate::gui::theme::{Palette, Theme, lerp_color, with_alpha};
use crate::gui::widgets::{clickable, flat_card_frame, ghost_button, segment_tabs, toggle_switch};
use crate::i18n::Locale;
#[cfg(all(feature = "mount", target_os = "windows"))]
use crate::mount_hints::WINFSP_RELEASES_URL;
#[cfg(feature = "mount")]
use crate::mount_hints::platform_prerequisite_hint;
use egui::{Frame, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

const COLUMN_MAX_W: f32 = 560.0;

pub(crate) fn show(app: &mut App, ui: &mut egui::Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    fill_background(ui, p.bg);
    egui::CentralPanel::default()
        .frame(Frame::new().fill(p.bg).inner_margin(egui::Margin {
            left: 24,
            right: 24,
            top: 16,
            bottom: 16,
        }))
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if ghost_button(ui, p, crate::ts!("settings.back")).clicked() {
                    events.push(Event::Back);
                }
                ui.add_space(10.0);
                ui.label(
                    RichText::new(crate::ts!("gui.settings.title"))
                        .size(20.0)
                        .strong()
                        .color(p.text),
                );
            });
            ui.add_space(18.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        let width = ui.available_width().min(COLUMN_MAX_W);
                        ui.set_max_width(width);
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            settings_column(app, ui, events);
                            ui.add_space(24.0);
                        });
                    });
                });
        });
    // Escape leaves the page.
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && ui.memory(|mem| mem.focused().is_none()) {
        events.push(Event::Back);
    }
}

/// Card with an icon + title header row.
fn settings_card(ui: &mut Ui, p: Palette, icon: Icon, title: String, body: impl FnOnce(&mut Ui)) {
    flat_card_frame(p).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
            ui.painter().rect_filled(rect, 8.0, p.accent_soft);
            paint_icon(ui, icon, rect.shrink(7.0), p.accent);
            ui.add_space(2.0);
            ui.label(RichText::new(title).size(14.0).strong().color(p.text));
        });
        ui.add_space(12.0);
        body(ui);
    });
    ui.add_space(12.0);
}

fn note(ui: &mut Ui, p: Palette, text: impl Into<String>) {
    ui.add(egui::Label::new(RichText::new(text.into()).size(11.0).color(p.muted)).wrap());
}

fn settings_column(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>) {
    let p = app.palette;

    settings_card(ui, p, Icon::Eye, crate::ts!("settings.display"), |ui| {
        ui.label(
            RichText::new(crate::ts!("settings.theme"))
                .size(12.0)
                .color(p.muted),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let half = (ui.available_width() - 10.0) * 0.5;
            for theme in [Theme::Cave, Theme::Parchment] {
                if theme_swatch(ui, p, theme, app.theme == theme, half).clicked()
                    && app.theme != theme
                {
                    events.push(Event::SetTheme(theme));
                }
            }
        });
    });

    settings_card(ui, p, Icon::Settings, crate::ts!("settings.locale"), |ui| {
        let labels: Vec<String> = Locale::ALL.iter().map(|locale| locale.label()).collect();
        let selected = Locale::ALL
            .iter()
            .position(|locale| *locale == app.locale)
            .unwrap_or(0);
        if let Some(index) = segment_tabs(ui, p, selected, &labels, 1.0)
            && let Some(locale) = Locale::ALL.get(index)
            && *locale != app.locale
        {
            events.push(Event::SetLocale(*locale));
        }
    });

    settings_card(ui, p, Icon::Shield, crate::ts!("settings.security"), |ui| {
        let mut enabled = app.auto_lock_enabled;
        if toggle_switch(
            ui,
            p,
            &mut enabled,
            crate::ts!("gui.settings.automatic_lock"),
        ) {
            events.push(Event::SetAutoLockEnabled(enabled));
        }
        ui.add_space(4.0);
        note(ui, p, crate::ts!("gui.settings.auto_lock_note"));
        if app.auto_lock_enabled {
            ui.add_space(10.0);
            let mut minutes = app.auto_lock_minutes;
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(crate::ts!("gui.settings.timeout_minutes"))
                        .size(12.5)
                        .color(p.muted),
                );
                ui.add_space(6.0);
                ui.spacing_mut().slider_width = (ui.available_width() - 60.0).max(80.0);
                if ui
                    .add(
                        egui::Slider::new(&mut minutes, 1..=60)
                            .show_value(true)
                            .trailing_fill(true),
                    )
                    .changed()
                {
                    events.push(Event::SetAutoLockMinutes(minutes));
                }
            });
        }
        ui.add_space(14.0);
        let mut anti_capture = app.anti_capture_enabled;
        if toggle_switch(
            ui,
            p,
            &mut anti_capture,
            crate::ts!("gui.settings.anti_capture"),
        ) {
            events.push(Event::SetAntiCapture(anti_capture));
        }
        ui.add_space(4.0);
        note(ui, p, super::super::anti_capture::platform_support_note());
        ui.add_space(10.0);
        note(ui, p, crate::ts!("gui.settings.tray_note"));
    });

    settings_card(ui, p, Icon::Download, crate::ts!("settings.files"), |ui| {
        note(ui, p, crate::ts!("gui.settings.extract_note"));
    });

    if app.is_unlocked() {
        settings_card(
            ui,
            p,
            Icon::HardDrive,
            crate::ts!("gui.settings.maintenance"),
            |ui| {
                note(ui, p, crate::ts!("gui.settings.maintenance_note"));
                ui.add_space(10.0);
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                    if ghost_button(ui, p, crate::ts!("gui.settings.verify")).clicked() {
                        events.push(Event::VerifyVault);
                    }
                    if ghost_button(ui, p, crate::ts!("gui.settings.defrag")).clicked() {
                        events.push(Event::DefragVault);
                    }
                    if ghost_button(ui, p, crate::ts!("gui.settings.recover")).clicked() {
                        events.push(Event::RecoverVault);
                    }
                    if ghost_button(ui, p, crate::ts!("gui.settings.change_password")).clicked() {
                        events.push(Event::BeginChangePassword);
                    }
                    if ghost_button(ui, p, crate::ts!("gui.settings.grow")).clicked() {
                        events.push(Event::BeginGrow);
                    }
                });
            },
        );
    }

    settings_card(ui, p, Icon::Mount, crate::ts!("settings.mounting"), |ui| {
        #[cfg(all(feature = "mount", target_os = "windows"))]
        {
            let detected = crate::mount::winfsp_runtime_detected();
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                crate::gui::widgets::status_dot(
                    ui,
                    dot.center(),
                    if detected { p.success } else { p.danger },
                    false,
                );
                let text = crate::gui::widgets::strip_status_glyph(&if detected {
                    crate::ts!("settings.winfsp_detected")
                } else {
                    crate::ts!("settings.winfsp_missing")
                })
                .to_owned();
                ui.label(RichText::new(text).size(12.5).color(if detected {
                    p.text
                } else {
                    p.danger
                }));
            });
            if !detected {
                ui.add_space(6.0);
                note(ui, p, platform_prerequisite_hint());
                ui.add_space(4.0);
                ui.hyperlink_to(crate::ts!("settings.winfsp_releases"), WINFSP_RELEASES_URL);
            }
        }
        #[cfg(all(feature = "mount", not(target_os = "windows")))]
        {
            note(ui, p, platform_prerequisite_hint());
        }
        #[cfg(not(feature = "mount"))]
        {
            note(ui, p, crate::ts!("settings.mount_not_compiled"));
        }
    });
}

/// Miniature preview of a theme: window chrome, sidebar, two rows, accent dot.
fn theme_swatch(ui: &mut Ui, p: Palette, theme: Theme, active: bool, width: f32) -> egui::Response {
    let target = theme.palette();
    let (rect, response) = ui.allocate_exact_size(vec2(width, 96.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover =
            ui.ctx()
                .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
        let on = ui
            .ctx()
            .animate_bool_with_time(response.id.with("on"), active, 0.18);
        let border = lerp_color(lerp_color(p.border, p.border_strong, hover), p.accent, on);
        ui.painter().rect_filled(rect, 10.0, p.surface_elevated);
        ui.painter().rect_stroke(
            rect,
            10.0,
            Stroke::new(1.0 + on, border),
            StrokeKind::Inside,
        );
        // Preview window.
        let preview = Rect::from_min_max(
            rect.min + vec2(12.0, 12.0),
            pos2(rect.right() - 12.0, rect.bottom() - 30.0),
        );
        ui.painter().rect_filled(preview, 6.0, target.bg);
        ui.painter()
            .rect_stroke(preview, 6.0, (1.0, target.border), StrokeKind::Inside);
        let side = Rect::from_min_max(
            preview.min,
            pos2(preview.left() + preview.width() * 0.3, preview.bottom()),
        );
        ui.painter().rect_filled(side, 6.0, target.surface);
        for i in 0..2 {
            let row = Rect::from_min_size(
                pos2(side.right() + 8.0, preview.top() + 10.0 + i as f32 * 14.0),
                vec2(preview.width() * 0.5, 6.0),
            );
            ui.painter().rect_filled(
                row,
                3.0,
                if i == 0 {
                    target.accent
                } else {
                    target.border_strong
                },
            );
        }
        ui.painter().circle_filled(
            pos2(side.center().x, preview.top() + 14.0),
            4.0,
            target.accent,
        );
        // Caption + check.
        let label = theme.label();
        ui.painter().text(
            pos2(rect.left() + 14.0, rect.bottom() - 15.0),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.0),
            lerp_color(p.muted, p.text, on.max(hover)),
        );
        if on > 0.01 {
            let c = pos2(rect.right() - 20.0, rect.bottom() - 15.0);
            ui.painter().circle_filled(c, 8.0 * on, p.accent);
            let stroke = Stroke::new(1.8, with_alpha(p.accent_text, on));
            ui.painter()
                .line_segment([c + vec2(-3.5, 0.0), c + vec2(-1.0, 2.8)], stroke);
            ui.painter()
                .line_segment([c + vec2(-1.0, 2.8), c + vec2(4.0, -3.0)], stroke);
        }
    }
    clickable(response)
}
