//! Locked (open/create vault) page — fully responsive, no fixed canvas.
//!
//! The hero breathes: a soft accent glow pulses behind the seal, and a ring
//! of ticks rotates slowly around the logo. Errors shake the card.

use super::Event;
use crate::gui::app::App;
use crate::gui::icons::Icon;
use crate::gui::layout::{content_scale, fill_background};
use crate::gui::theme::{Palette, paint_radial_glow, paint_vertical_gradient, with_alpha};
use crate::gui::widgets::{
    card_frame, check_box, clickable, field_label, icon_button, input_field, locale_picker,
    primary_button, relative_time, segment_tabs, show_toast, vault_display_name,
};
use egui::{Align, Frame, Layout, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use std::time::Duration;

const CARD_MAX_W: f32 = 440.0;
const SHAKE: Duration = Duration::from_millis(480);

pub(crate) fn show(app: &mut App, ui: &mut egui::Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    fill_background(ui, p.bg);
    // Ambient: subtle top-to-bottom lift plus a breathing glow behind the hero.
    let screen = ui.max_rect();
    paint_vertical_gradient(ui.painter(), screen, p.bg_raised, p.bg);
    let time = ui.input(|i| i.time) as f32;
    let breath = (time * 0.9).sin() * 0.5 + 0.5;
    let glow_center = pos2(screen.center().x, screen.top() + screen.height() * 0.30);
    paint_radial_glow(
        ui.painter(),
        glow_center,
        screen.width().min(screen.height()) * (0.46 + 0.05 * breath),
        with_alpha(p.accent_glow, 0.75 + 0.25 * breath),
        1.6,
    );
    ui.ctx().request_repaint_after(Duration::from_millis(33));

    egui::CentralPanel::default()
        .frame(Frame::new().inner_margin(egui::Margin::symmetric(20, 12)))
        .show_inside(ui, |ui| {
            top_bar(app, ui, events);
            let scale = content_scale(ui);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() * 0.02).clamp(2.0, 24.0));
                        brand_hero(app, ui, scale, time);
                        ui.add_space((16.0 * scale).clamp(10.0, 28.0));
                        main_card(app, ui, events, scale);
                        ui.add_space(16.0);
                    });
                });
        });
    show_toast(ui, app);
}

fn top_bar(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>) {
    let p = app.palette;
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(locale) = locale_picker(ui, p, app.locale, 1.0) {
                events.push(Event::SetLocale(locale));
            }
            ui.add_space(6.0);
            if theme_toggle(ui, p).clicked() {
                events.push(Event::ToggleTheme);
            }
        });
    });
}

/// Sun/moon toggle: a disc that morphs between a crescent and a full sun.
pub(crate) fn theme_toggle(ui: &mut Ui, p: Palette) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::click());
    let hover =
        ui.ctx()
            .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
    if hover > 0.0 {
        ui.painter()
            .rect_filled(rect, 8.0, with_alpha(p.surface_elevated, hover));
    }
    let color = crate::gui::theme::lerp_color(p.muted, p.accent, hover);
    let c = rect.center();
    let r = 6.5;
    // lightness 0 → moon, 1 → sun
    let sun = p.lightness;
    ui.painter().circle_filled(c, r, color);
    if sun < 0.99 {
        // Bite out a crescent with a bg-colored disc.
        let offset = egui::lerp(3.2..=12.0, sun);
        ui.painter()
            .circle_filled(c + vec2(offset, -2.5), r * 0.92, p.bg);
    }
    if sun > 0.01 {
        for i in 0..8 {
            let angle = i as f32 / 8.0 * std::f32::consts::TAU;
            let dir = vec2(angle.cos(), angle.sin());
            let a = c + dir * (r + 2.5);
            let b = c + dir * (r + 2.5 + 2.6 * sun);
            ui.painter()
                .line_segment([a, b], Stroke::new(1.4, with_alpha(color, sun)));
        }
    }
    clickable(response)
}

/// Centered brand block: seal with rotating ticks, letterspaced wordmark,
/// hairline rule, tagline.
fn brand_hero(app: &App, ui: &mut Ui, scale: f32, time: f32) {
    let p = app.palette;
    let logo_size = 44.0 * scale;
    let (hero_rect, _) =
        ui.allocate_exact_size(vec2(logo_size * 2.6, logo_size * 2.0), Sense::hover());
    let center = hero_rect.center();
    let logo_rect = Rect::from_center_size(center, vec2(logo_size, logo_size));
    egui::Image::new(crate::gui::LOGO.clone())
        .fit_to_exact_size(logo_rect.size())
        .paint_at(ui, logo_rect);

    let ring = logo_size * 0.72;
    ui.painter()
        .circle_stroke(center, ring, Stroke::new((1.5 * scale).max(1.0), p.accent));
    ui.painter().circle_stroke(
        center,
        ring + 5.0 * scale,
        Stroke::new(1.0, with_alpha(p.accent, 0.28)),
    );
    // Rotating tick ring — the "seal" slowly turning.
    let tick_r = ring + 11.0 * scale;
    let ticks = 36;
    let spin = time * 0.25;
    for i in 0..ticks {
        let angle = i as f32 / ticks as f32 * std::f32::consts::TAU + spin;
        let dir = vec2(angle.cos(), angle.sin());
        let long = i % 6 == 0;
        let len = if long { 4.5 } else { 2.0 } * scale;
        let alpha = if long { 0.7 } else { 0.32 };
        ui.painter().line_segment(
            [center + dir * tick_r, center + dir * (tick_r + len)],
            Stroke::new(1.0, with_alpha(p.accent, alpha)),
        );
    }

    ui.add_space(2.0 * scale);
    ui.label(
        RichText::new(letterspaced(&crate::ts!("gui.locked.title")))
            .size(21.0 * scale)
            .color(p.accent)
            .strong(),
    );
    ui.add_space(6.0 * scale);
    let (rule, _) = ui.allocate_exact_size(vec2(140.0 * scale, 1.0), Sense::hover());
    paint_rule(ui, rule, p);
    ui.add_space(6.0 * scale);
    ui.label(
        RichText::new(crate::ts!("gui.locked.subtitle"))
            .size(11.5 * scale)
            .color(p.muted),
    );
}

/// Hairline that fades out toward both ends.
fn paint_rule(ui: &Ui, rect: Rect, p: Palette) {
    let mid = rect.center().x;
    let left = Rect::from_min_max(rect.min, pos2(mid, rect.max.y));
    let right = Rect::from_min_max(pos2(mid, rect.min.y), rect.max);
    let mut mesh = egui::Mesh::default();
    let edge = with_alpha(p.accent, 0.0);
    let core = with_alpha(p.accent, 0.7);
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

/// "Closed Sesame" -> "C L O S E D   S E S A M E"
fn letterspaced(title: &str) -> String {
    title
        .split_whitespace()
        .map(|word| {
            word.chars()
                .flat_map(|ch| ch.to_uppercase())
                .map(|ch| ch.to_string())
                .collect::<Vec<_>>()
                .join("\u{2009}")
        })
        .collect::<Vec<_>>()
        .join("\u{2003}")
}

/// Horizontal offset for the error shake, decaying over [`SHAKE`].
fn shake_offset(app: &App) -> f32 {
    let Some(at) = app.error_pulse else {
        return 0.0;
    };
    let age = at.elapsed();
    if age >= SHAKE {
        return 0.0;
    }
    let t = age.as_secs_f32() / SHAKE.as_secs_f32();
    (t * std::f32::consts::TAU * 3.5).sin() * 7.0 * (1.0 - t)
}

fn main_card(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>, scale: f32) {
    let p = app.palette;
    let width = CARD_MAX_W.min(ui.available_width());
    let shake = shake_offset(app);
    let (outer, _) = ui.allocate_exact_size(vec2(width, 0.0), Sense::hover());
    let card_rect = Rect::from_min_size(pos2(outer.left() + shake, outer.top()), vec2(width, 0.0));
    // `scope_builder` allocates the child's final rect in the parent, so the
    // page reserves exactly the height the card used.
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(
                card_rect.min,
                pos2(card_rect.max.x, f32::INFINITY),
            ))
            .layout(Layout::top_down(Align::Min)),
        |ui| {
            ui.set_width(width);
            card_frame(p).inner_margin(18.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let open_label = crate::ts!("gui.locked.section_open");
                let create_label = crate::ts!("gui.locked.section_create");
                if let Some(index) = segment_tabs(
                    ui,
                    p,
                    usize::from(app.create_mode),
                    &[&open_label, &create_label],
                    scale,
                ) {
                    let create_mode = index == 1;
                    if create_mode != app.create_mode {
                        app.create_mode = create_mode;
                        app.status.clear();
                    }
                }
                ui.add_space(14.0 * scale);
                if app.create_mode {
                    create_form(app, ui, events, scale);
                } else {
                    open_form(app, ui, events, scale);
                }
            });
        },
    );
}

/// Enter inside any field submits the form.
fn enter_pressed(ui: &Ui, responses: &[&egui::Response]) -> bool {
    responses.iter().any(|response| {
        response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
    })
}

fn open_form(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>, scale: f32) {
    let p = app.palette;
    field_label(ui, p, crate::ts!("gui.locked.path_label"), scale);
    ui.add_space(6.0 * scale);
    let mut path_response = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0 * scale;
        let button_size = 40.0 * scale;
        let input_width = ui.available_width() - button_size - 8.0 * scale;
        path_response = Some(crate::gui::widgets::input_field_sized(
            ui,
            p,
            &mut app.path_input,
            crate::ts!("gui.locked.path_hint"),
            Some(Icon::FolderLock),
            false,
            input_width,
            button_size,
            scale,
        ));
        if icon_button(ui, p, Icon::FolderOpen, button_size)
            .on_hover_text(crate::ts!("gui.locked.browse"))
            .clicked()
        {
            events.push(Event::BrowseVault);
        }
    });

    ui.add_space(12.0 * scale);
    field_label(ui, p, crate::ts!("gui.locked.password_hint"), scale);
    ui.add_space(6.0 * scale);
    let password_response = ui
        .add_enabled_ui(!app.no_password, |ui| {
            input_field(
                ui,
                p,
                &mut app.password,
                crate::ts!("gui.locked.password_hint"),
                Some(Icon::Key),
                true,
                scale,
            )
        })
        .inner;

    ui.add_space(10.0 * scale);
    let mut no_password = app.no_password;
    if check_box(
        ui,
        p,
        &mut no_password,
        crate::ts!("gui.locked.no_password"),
    ) {
        events.push(Event::SetNoPassword(no_password));
    }

    ui.add_space(16.0 * scale);
    let submit = primary_button(
        ui,
        p,
        Some(Icon::Lock),
        crate::ts!("gui.locked.open_vault"),
        scale,
    )
    .clicked();
    let enter = path_response
        .as_ref()
        .is_some_and(|r| enter_pressed(ui, &[r, &password_response]));
    if submit || enter {
        events.push(Event::Open);
    }

    if !app.recent.entries.is_empty() {
        ui.add_space(16.0 * scale);
        crate::gui::widgets::hairline(ui, p);
        ui.add_space(12.0 * scale);
        field_label(ui, p, crate::ts!("gui.locked.recent"), scale);
        ui.add_space(6.0 * scale);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        egui::ScrollArea::vertical()
            .max_height(150.0 * scale)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for recent in app.recent.entries.clone() {
                    let path = recent.path.display().to_string();
                    let age = now.saturating_sub(recent.last_opened);
                    if recent_row(ui, p, &path, &relative_time(age)).clicked() {
                        events.push(Event::OpenRecent(path));
                    }
                    ui.add_space(2.0);
                }
            });
    }
}

/// Recent-vault row: drive icon, file name, dimmed parent path, time ago.
fn recent_row(ui: &mut Ui, p: Palette, path: &str, when: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover =
            ui.ctx()
                .animate_bool_with_time(response.id.with("hover"), response.hovered(), 0.12);
        ui.painter()
            .rect_filled(rect, 8.0, with_alpha(p.surface_elevated, hover));
        let icon_rect =
            Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), vec2(16.0, 16.0));
        crate::gui::icons::paint_icon(
            ui,
            Icon::HardDrive,
            icon_rect,
            crate::gui::theme::lerp_color(p.muted, p.accent, hover),
        );
        let name = vault_display_name(path);
        let parent = std::path::Path::new(path)
            .parent()
            .map(|parent| parent.display().to_string())
            .unwrap_or_default();
        let text_x = rect.left() + 40.0;
        let right_w = 70.0;
        let max_w = (rect.width() - 40.0 - right_w - 8.0).max(40.0);
        let name_galley =
            ui.painter()
                .layout(name, egui::FontId::proportional(13.0), p.text, max_w);
        let parent_galley =
            ui.painter()
                .layout(parent, egui::FontId::proportional(10.5), p.faint, max_w);
        ui.painter()
            .galley(pos2(text_x, rect.top() + 7.0), name_galley, p.text);
        ui.painter()
            .galley(pos2(text_x, rect.top() + 25.0), parent_galley, p.faint);
        let when_galley =
            ui.painter()
                .layout_no_wrap(when.to_owned(), egui::FontId::proportional(10.5), p.muted);
        ui.painter().galley(
            pos2(
                rect.right() - 12.0 - when_galley.size().x,
                rect.center().y - when_galley.size().y * 0.5,
            ),
            when_galley,
            p.muted,
        );
    }
    clickable(response)
}

fn create_form(app: &mut App, ui: &mut Ui, events: &mut Vec<Event>, scale: f32) {
    let p = app.palette;
    field_label(ui, p, crate::ts!("gui.locked.new_vault_name"), scale);
    ui.add_space(6.0 * scale);
    let name_response = input_field(
        ui,
        p,
        &mut app.create_name,
        "my-sesame",
        Some(Icon::File),
        false,
        scale,
    );

    ui.add_space(12.0 * scale);
    field_label(ui, p, crate::ts!("gui.locked.password_hint"), scale);
    ui.add_space(6.0 * scale);
    let password_response = ui
        .add_enabled_ui(!app.no_password, |ui| {
            input_field(
                ui,
                p,
                &mut app.password,
                crate::ts!("gui.locked.password_hint"),
                Some(Icon::Key),
                true,
                scale,
            )
        })
        .inner;
    if !app.no_password {
        ui.add_space(6.0 * scale);
        password_strength(ui, p, &app.password);
    }

    ui.add_space(10.0 * scale);
    let mut no_password = app.no_password;
    if check_box(
        ui,
        p,
        &mut no_password,
        crate::ts!("gui.locked.no_password"),
    ) {
        events.push(Event::SetNoPassword(no_password));
    }

    ui.add_space(12.0 * scale);
    field_label(ui, p, crate::ts!("gui.locked.create_size"), scale);
    ui.add_space(6.0 * scale);
    let size_response = input_field(
        ui,
        p,
        &mut app.create_size,
        "500MB",
        Some(Icon::Shield),
        false,
        scale,
    );
    ui.add_space(6.0 * scale);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for preset in ["100MB", "500MB", "2GB", "10GB"] {
            let active = app.create_size.trim().eq_ignore_ascii_case(preset);
            let tone = if active { p.accent } else { p.muted };
            if crate::gui::widgets::chip_button(ui, p, None, preset, tone).clicked() {
                app.create_size = preset.to_owned();
            }
        }
    });

    ui.add_space(16.0 * scale);
    let submit = primary_button(
        ui,
        p,
        Some(Icon::Plus),
        crate::ts!("gui.locked.create"),
        scale,
    )
    .clicked();
    if submit || enter_pressed(ui, &[&name_response, &password_response, &size_response]) {
        events.push(Event::Create);
    }
}

/// Four-segment strength meter; purely heuristic (length + character classes).
fn password_strength(ui: &mut Ui, p: Palette, password: &str) {
    let score = strength_score(password);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
    let gap = 4.0;
    let seg_w = (rect.width() - gap * 3.0) / 4.0;
    let tone = match score {
        0 => p.border_strong,
        1 => p.danger,
        2 => p.warning,
        3 => p.accent,
        _ => p.success,
    };
    for i in 0..4 {
        let seg = Rect::from_min_size(
            pos2(rect.left() + i as f32 * (seg_w + gap), rect.top()),
            vec2(seg_w, rect.height()),
        );
        let lit = ui
            .ctx()
            .animate_bool_with_time(ui.id().with("pw_seg").with(i), i < score, 0.2);
        ui.painter().rect_filled(
            seg,
            2.0,
            crate::gui::theme::lerp_color(p.surface_elevated, tone, lit),
        );
    }
}

fn strength_score(password: &str) -> usize {
    if password.is_empty() {
        return 0;
    }
    let mut classes = 0;
    classes += usize::from(password.chars().any(|c| c.is_ascii_lowercase()));
    classes += usize::from(password.chars().any(|c| c.is_ascii_uppercase()));
    classes += usize::from(password.chars().any(|c| c.is_ascii_digit()));
    classes += usize::from(password.chars().any(|c| !c.is_ascii_alphanumeric()));
    let len = password.chars().count();
    let mut score = 1;
    if len >= 8 && classes >= 2 {
        score = 2;
    }
    if len >= 12 && classes >= 3 {
        score = 3;
    }
    if len >= 16 && classes >= 3 || len >= 20 {
        score = 4;
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_is_monotone_on_examples() {
        assert_eq!(strength_score(""), 0);
        assert_eq!(strength_score("abc"), 1);
        assert_eq!(strength_score("abcdefG1"), 2);
        assert_eq!(strength_score("abcdefG1!xyz"), 3);
        assert_eq!(strength_score("abcdefG1!xyzQWER"), 4);
    }
}
