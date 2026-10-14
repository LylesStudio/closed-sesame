//! First-run language picker — responsive centered card over the brand glow.

use super::Event;
use crate::gui::app::App;
use crate::gui::layout::{content_scale, fill_background};
use crate::gui::theme::{
    Palette, lerp_color, paint_radial_glow, paint_vertical_gradient, with_alpha,
};
use crate::gui::widgets::{card_frame, clickable, paint_chevron};
use crate::i18n::Locale;
use egui::{Frame, RichText, Sense, StrokeKind, Ui, pos2, vec2};

const LOCALE_OPTIONS: [(Locale, &str, &str); 3] = [
    (Locale::En, "English", "Continue in English"),
    (Locale::ZhCn, "简体中文", "使用简体中文继续"),
    (Locale::Ja, "日本語", "日本語で続ける"),
];

pub(crate) fn show(app: &App, ui: &mut egui::Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    fill_background(ui, p.bg);
    let screen = ui.max_rect();
    paint_vertical_gradient(ui.painter(), screen, p.bg_raised, p.bg);
    let time = ui.input(|i| i.time) as f32;
    let breath = (time * 0.9).sin() * 0.5 + 0.5;
    paint_radial_glow(
        ui.painter(),
        pos2(screen.center().x, screen.top() + screen.height() * 0.32),
        screen.width().min(screen.height()) * (0.44 + 0.04 * breath),
        with_alpha(p.accent_glow, 0.7 + 0.3 * breath),
        1.6,
    );
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(33));

    let scale = content_scale(ui);
    egui::CentralPanel::default()
        .frame(Frame::new())
        .show_inside(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.14).clamp(20.0, 110.0));
                let logo = 56.0 * scale;
                let (logo_rect, _) = ui.allocate_exact_size(vec2(logo, logo), Sense::hover());
                egui::Image::new(crate::gui::LOGO.clone())
                    .fit_to_exact_size(logo_rect.size())
                    .paint_at(ui, logo_rect);
                ui.painter().circle_stroke(
                    logo_rect.center(),
                    logo * 0.68,
                    egui::Stroke::new(1.5, p.accent),
                );
                ui.add_space(14.0 * scale);
                ui.label(
                    RichText::new("Closed Sesame")
                        .size(24.0 * scale)
                        .color(p.accent)
                        .strong(),
                );
                ui.add_space(6.0 * scale);
                ui.label(
                    RichText::new("Choose a language · 选择语言 · 言語を選択")
                        .size(12.0 * scale)
                        .color(p.muted),
                );
                ui.add_space(22.0 * scale);
                ui.set_max_width(400.0_f32.min(ui.available_width() - 32.0));
                card_frame(p).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 8.0;
                    for (locale, title, subtitle) in LOCALE_OPTIONS {
                        if language_row(ui, p, title, subtitle).clicked() {
                            events.push(Event::SetLocale(locale));
                        }
                    }
                });
            });
        });
}

fn language_row(ui: &mut Ui, p: Palette, title: &str, subtitle: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover =
            ui.ctx()
                .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
        ui.painter().rect_filled(
            rect,
            10.0,
            lerp_color(p.surface_elevated, with_alpha(p.accent_soft, 1.0), hover),
        );
        ui.painter().rect_stroke(
            rect,
            10.0,
            (1.0, lerp_color(p.border, p.accent, hover)),
            StrokeKind::Inside,
        );
        ui.painter().text(
            pos2(rect.left() + 16.0, rect.top() + 19.0),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(15.0),
            lerp_color(p.text, p.accent, hover),
        );
        ui.painter().text(
            pos2(rect.left() + 16.0, rect.top() + 39.0),
            egui::Align2::LEFT_CENTER,
            subtitle,
            egui::FontId::proportional(11.5),
            p.muted,
        );
        let chevron_x = rect.right() - 18.0 + hover * 3.0;
        paint_chevron(
            ui.painter(),
            pos2(chevron_x, rect.center().y),
            11.0,
            lerp_color(p.faint, p.accent, hover),
        );
    }
    clickable(response)
}
