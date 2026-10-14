//! Shared egui building blocks for the Closed Sesame GUI.
//!
//! Every widget takes the frame's [`Palette`] so colors follow the animated
//! theme cross-fade. Hover/press states are animated via egui's
//! `animate_bool_with_time` so interactions feel fluid rather than binary.

use super::app::{App, NotificationKind};
use super::icons::{Icon, paint_icon};
use super::theme::{Palette, ease_out, lerp_color, with_alpha};
use crate::i18n::Locale;
use egui::{
    Align, Align2, Color32, FontId, Frame, Layout, Pos2, Rect, Response, RichText, Sense, Stroke,
    StrokeKind, TextEdit, Ui, Vec2, pos2, vec2,
};

pub(crate) fn clickable(response: Response) -> Response {
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn hover_t(ui: &Ui, response: &Response, salt: &str) -> f32 {
    ui.ctx()
        .animate_bool_with_time(response.id.with(salt), response.hovered(), 0.12)
}

fn press_t(ui: &Ui, response: &Response, salt: &str) -> f32 {
    ui.ctx().animate_bool_with_time(
        response.id.with(salt),
        response.is_pointer_button_down_on(),
        0.08,
    )
}

fn decorative_label(ui: &mut Ui, text: impl Into<egui::WidgetText>) -> Response {
    ui.add(egui::Label::new(text).sense(Sense::empty()))
}

// ---------------------------------------------------------------------------
// Frames & text
// ---------------------------------------------------------------------------

pub(crate) fn card_frame(p: Palette) -> Frame {
    Frame::new()
        .fill(p.surface)
        .stroke((1.0, p.border))
        .corner_radius(p.card_radius())
        .inner_margin(20.0)
        .shadow(egui::epaint::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: with_alpha(p.shadow, 0.55),
        })
}

pub(crate) fn flat_card_frame(p: Palette) -> Frame {
    Frame::new()
        .fill(p.surface)
        .stroke((1.0, p.border))
        .corner_radius(p.card_radius())
        .inner_margin(16.0)
}

pub(crate) fn section_label(ui: &mut Ui, p: Palette, text: impl AsRef<str>) {
    let text = text.as_ref();
    let is_ascii = text.is_ascii();
    let shown = if is_ascii {
        text.to_uppercase()
    } else {
        text.to_owned()
    };
    let mut job = egui::text::LayoutJob::default();
    let mut format = egui::TextFormat::simple(egui::FontId::proportional(10.5), p.muted);
    // Section headers get a little tracking; CJK text is left untouched.
    format.extra_letter_spacing = if is_ascii { 1.4 } else { 0.0 };
    job.append(&shown, 0.0, format);
    ui.add(egui::Label::new(job).selectable(false));
}

pub(crate) fn hairline(ui: &mut Ui, p: Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, p.border);
}

pub(crate) fn field_label(ui: &mut Ui, p: Palette, text: impl AsRef<str>, scale: f32) {
    decorative_label(
        ui,
        RichText::new(text.as_ref())
            .size(11.5 * scale)
            .strong()
            .color(p.muted),
    );
}

// ---------------------------------------------------------------------------
// Buttons
// ---------------------------------------------------------------------------

/// Full-width primary action with icon, hover brightening and press dip.
pub(crate) fn primary_button(
    ui: &mut Ui,
    p: Palette,
    icon: Option<Icon>,
    label: impl AsRef<str>,
    scale: f32,
) -> Response {
    primary_button_sized(
        ui,
        p,
        icon,
        label,
        ui.available_width(),
        40.0 * scale,
        scale,
    )
}

pub(crate) fn primary_button_sized(
    ui: &mut Ui,
    p: Palette,
    icon: Option<Icon>,
    label: impl AsRef<str>,
    width: f32,
    height: f32,
    scale: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = hover_t(ui, &response, "hover");
        let press = press_t(ui, &response, "press");
        let lift = if p.is_dark() { 1.1 } else { 1.18 };
        let fill = lerp_color(p.primary_fill, p.primary_fill.gamma_multiply(lift), hover);
        let fill = lerp_color(fill, p.primary_fill.gamma_multiply(0.82), press);
        let rect = rect.shrink(press * 1.0);
        let radius = 9.0 * scale;
        // Glow under the button in the dark theme.
        if p.is_dark() && hover > 0.0 {
            ui.painter().rect_filled(
                rect.expand(2.0 + hover * 2.0),
                radius + 2.0,
                with_alpha(p.accent, 0.10 * hover),
            );
        }
        ui.painter().rect_filled(rect, radius, fill);
        // Top sheen.
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(
                1.0,
                with_alpha(Color32::WHITE, if p.is_dark() { 0.18 } else { 0.08 }),
            ),
            StrokeKind::Inside,
        );
        paint_icon_label_centered(
            ui,
            rect,
            icon,
            label.as_ref(),
            13.5 * scale,
            p.accent_text,
            true,
        );
    }
    clickable(response)
}

/// Solid button with custom fill (e.g. danger confirm).
pub(crate) fn solid_button(
    ui: &mut Ui,
    label: impl AsRef<str>,
    fill: Color32,
    text: Color32,
    min_width: f32,
) -> Response {
    let galley =
        ui.painter()
            .layout_no_wrap(label.as_ref().to_owned(), FontId::proportional(13.0), text);
    let desired = vec2((galley.size().x + 28.0).max(min_width), 36.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = hover_t(ui, &response, "hover");
        let color = lerp_color(fill, fill.gamma_multiply(1.12), hover);
        ui.painter().rect_filled(rect, 8.0, color);
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, text);
    }
    clickable(response)
}

/// Outlined, low-emphasis button; border and text warm up on hover.
pub(crate) fn ghost_button(ui: &mut Ui, p: Palette, label: impl AsRef<str>) -> Response {
    ghost_button_icon(ui, p, None, label)
}

pub(crate) fn ghost_button_icon(
    ui: &mut Ui,
    p: Palette,
    icon: Option<Icon>,
    label: impl AsRef<str>,
) -> Response {
    let font = FontId::proportional(13.0);
    let galley = ui
        .painter()
        .layout_no_wrap(label.as_ref().to_owned(), font, p.muted);
    let icon_w = if icon.is_some() { 14.0 + 6.0 } else { 0.0 };
    let desired = vec2(galley.size().x + icon_w + 26.0, 34.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = hover_t(ui, &response, "hover");
        let border = lerp_color(p.border, p.accent, hover);
        let text = lerp_color(p.muted, p.text, hover);
        ui.painter()
            .rect_filled(rect, 8.0, with_alpha(p.surface_elevated, hover * 0.8));
        ui.painter()
            .rect_stroke(rect, 8.0, (1.0, border), StrokeKind::Inside);
        paint_icon_label_centered(ui, rect, icon, label.as_ref(), 13.0, text, false);
    }
    clickable(response)
}

/// Square icon-only button with border; hover accent.
pub(crate) fn icon_button(ui: &mut Ui, p: Palette, icon: Icon, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = hover_t(ui, &response, "hover");
        let border = lerp_color(p.border, p.accent, hover);
        let fg = lerp_color(p.muted, p.accent, hover);
        ui.painter()
            .rect_filled(rect, 8.0, lerp_color(p.surface, p.surface_elevated, hover));
        ui.painter()
            .rect_stroke(rect, 8.0, (1.0, border), StrokeKind::Inside);
        let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(size * 0.42));
        paint_icon(ui, icon, icon_rect, fg);
    }
    clickable(response)
}

/// Borderless icon button (toolbar style).
pub(crate) fn flat_icon_button(ui: &mut Ui, p: Palette, icon: Icon, size: f32) -> Response {
    flat_icon_button_colored(ui, p, icon, size, p.muted, p.text)
}

pub(crate) fn flat_icon_button_colored(
    ui: &mut Ui,
    p: Palette,
    icon: Icon,
    size: f32,
    idle: Color32,
    hot: Color32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = hover_t(ui, &response, "hover");
        if hover > 0.0 {
            ui.painter()
                .rect_filled(rect, 7.0, with_alpha(p.surface_elevated, hover));
        }
        let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(size * 0.5));
        paint_icon(ui, icon, icon_rect, lerp_color(idle, hot, hover));
    }
    clickable(response)
}

/// Compact text button used in toolbars and action bars.
pub(crate) fn chip_button(
    ui: &mut Ui,
    _p: Palette,
    icon: Option<Icon>,
    label: impl AsRef<str>,
    tone: Color32,
) -> Response {
    let font = FontId::proportional(12.5);
    let galley = ui
        .painter()
        .layout_no_wrap(label.as_ref().to_owned(), font, tone);
    let icon_w = if icon.is_some() { 13.0 + 5.0 } else { 0.0 };
    let desired = vec2(galley.size().x + icon_w + 20.0, 28.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = hover_t(ui, &response, "hover");
        ui.painter().rect_filled(
            rect,
            7.0,
            lerp_color(with_alpha(tone, 0.08), with_alpha(tone, 0.18), hover),
        );
        paint_icon_label_centered(ui, rect, icon, label.as_ref(), 12.5, tone, false);
    }
    clickable(response)
}

/// Sidebar navigation row: icon + label, hover wash, optional accent state.
pub(crate) fn nav_button(
    ui: &mut Ui,
    p: Palette,
    icon: Option<Icon>,
    label: impl AsRef<str>,
    width: f32,
    active: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 36.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = hover_t(ui, &response, "hover");
        let active_t = ui
            .ctx()
            .animate_bool_with_time(response.id.with("active"), active, 0.15);
        let bg = lerp_color(
            Color32::TRANSPARENT,
            p.surface_elevated,
            hover.max(active_t * 0.9),
        );
        ui.painter().rect_filled(rect, 8.0, bg);
        if active_t > 0.0 {
            let bar = Rect::from_min_size(
                pos2(rect.left(), rect.center().y - 8.0 * active_t),
                vec2(2.5, 16.0 * active_t),
            );
            ui.painter().rect_filled(bar, 2.0, p.accent);
        }
        let fg = lerp_color(p.text, p.accent, active_t);
        let icon_fg = lerp_color(p.muted, p.accent, active_t.max(hover * 0.6));
        let mut x = rect.left() + 12.0;
        if let Some(icon) = icon {
            let icon_rect =
                Rect::from_center_size(pos2(x + 8.0, rect.center().y), Vec2::splat(16.0));
            paint_icon(ui, icon, icon_rect, icon_fg);
            x += 16.0 + 10.0;
        }
        let galley =
            ui.painter()
                .layout_no_wrap(label.as_ref().to_owned(), FontId::proportional(13.0), fg);
        ui.painter()
            .galley(pos2(x, rect.center().y - galley.size().y * 0.5), galley, fg);
    }
    clickable(response)
}

/// Context-menu row.
pub(crate) fn menu_item(ui: &mut Ui, p: Palette, icon: Icon, label: impl AsRef<str>) -> Response {
    menu_item_colored(ui, p, icon, label, p.text)
}

pub(crate) fn menu_item_colored(
    ui: &mut Ui,
    p: Palette,
    icon: Icon,
    label: impl AsRef<str>,
    color: Color32,
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width().max(160.0), 30.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let hover = hover_t(ui, &response, "hover");
        if hover > 0.0 {
            ui.painter()
                .rect_filled(rect, 6.0, with_alpha(p.surface_elevated, hover));
        }
        let x = rect.left() + 8.0;
        let icon_rect = Rect::from_center_size(pos2(x + 7.0, rect.center().y), Vec2::splat(14.0));
        paint_icon(ui, icon, icon_rect, color);
        let galley = ui.painter().layout_no_wrap(
            label.as_ref().to_owned(),
            FontId::proportional(13.0),
            color,
        );
        ui.painter().galley(
            pos2(x + 14.0 + 10.0, rect.center().y - galley.size().y * 0.5),
            galley,
            color,
        );
    }
    clickable(response)
}

fn paint_icon_label_centered(
    ui: &mut Ui,
    rect: Rect,
    icon: Option<Icon>,
    label: &str,
    size: f32,
    color: Color32,
    strong: bool,
) {
    let font = FontId::proportional(size);
    let galley = if strong {
        let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), font, color);
        job.sections.iter_mut().for_each(|section| {
            section.format.extra_letter_spacing = 0.2;
        });
        ui.painter().layout_job(job)
    } else {
        ui.painter().layout_no_wrap(label.to_owned(), font, color)
    };
    let icon_size = size * 1.05;
    let gap = 7.0;
    let icon_w = icon.map(|_| icon_size + gap).unwrap_or(0.0);
    let total = icon_w + galley.size().x;
    let mut x = rect.center().x - total * 0.5;
    if let Some(icon) = icon {
        let icon_rect = Rect::from_center_size(
            pos2(x + icon_size * 0.5, rect.center().y),
            Vec2::splat(icon_size),
        );
        paint_icon(ui, icon, icon_rect, color);
        x += icon_w;
    }
    ui.painter().galley(
        pos2(x, rect.center().y - galley.size().y * 0.5),
        galley,
        color,
    );
}

// ---------------------------------------------------------------------------
// Inputs
// ---------------------------------------------------------------------------

/// Bordered single-line input with leading icon and animated focus ring.
/// Returns the inner `TextEdit` response (use `.lost_focus()` + Enter etc).
pub(crate) fn input_field(
    ui: &mut Ui,
    p: Palette,
    value: &mut String,
    placeholder: impl AsRef<str>,
    icon: Option<Icon>,
    is_password: bool,
    scale: f32,
) -> Response {
    input_field_sized(
        ui,
        p,
        value,
        placeholder,
        icon,
        is_password,
        ui.available_width(),
        40.0 * scale,
        scale,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn input_field_sized(
    ui: &mut Ui,
    p: Palette,
    value: &mut String,
    placeholder: impl AsRef<str>,
    icon: Option<Icon>,
    is_password: bool,
    width: f32,
    height: f32,
    scale: f32,
) -> Response {
    let (rect, outer) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let id = outer.id.with("text_edit");
    let has_focus = ui.ctx().memory(|mem| mem.has_focus(id));
    let focus = ui
        .ctx()
        .animate_bool_with_time(id.with("focus"), has_focus, 0.15);
    let hover = ui
        .ctx()
        .animate_bool_with_time(id.with("hover"), outer.hovered(), 0.12);

    let mut text_response = None;
    if ui.is_rect_visible(rect) {
        let radius = 8.0;
        if focus > 0.0 {
            ui.painter().rect_stroke(
                rect.expand(2.0 * focus),
                radius + 2.0,
                Stroke::new(2.0, with_alpha(p.accent, 0.28 * focus)),
                StrokeKind::Outside,
            );
        }
        ui.painter().rect_filled(rect, radius, p.surface_elevated);
        let border = lerp_color(
            lerp_color(p.border, p.border_strong, hover),
            p.accent,
            focus,
        );
        ui.painter()
            .rect_stroke(rect, radius, (1.0, border), StrokeKind::Inside);

        let inner = rect.shrink2(vec2(12.0 * scale, 0.0));
        let icon_w = if icon.is_some() { 24.0 * scale } else { 0.0 };
        if let Some(icon) = icon {
            let icon_rect = Rect::from_center_size(
                pos2(inner.left() + 7.0 * scale, inner.center().y),
                Vec2::splat(14.0 * scale),
            );
            paint_icon(ui, icon, icon_rect, lerp_color(p.muted, p.accent, focus));
        }
        let text_rect = Rect::from_min_max(pos2(inner.left() + icon_w, inner.top()), inner.max);
        ui.scope_builder(egui::UiBuilder::new().max_rect(text_rect), |ui| {
            ui.set_height(text_rect.height());
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let edit = TextEdit::singleline(value)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY)
                    .margin(egui::Margin::ZERO)
                    .font(FontId::proportional(13.5 * scale))
                    .text_color(p.text)
                    .hint_text(
                        RichText::new(placeholder.as_ref())
                            .color(p.faint)
                            .size(13.5 * scale),
                    )
                    .password(is_password);
                text_response = Some(ui.add(edit));
            });
        });
    }
    text_response.unwrap_or(outer)
}

/// Lay out `text` on one line, eliding with `?` when wider than `max_width`.
pub(crate) fn truncated_galley(
    ui: &Ui,
    text: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap.max_width = max_width;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('\u{2026}');
    ui.painter().layout_job(job)
}

/// Animated iOS-style switch with trailing label. Returns `true` on toggle.
pub(crate) fn toggle_switch(
    ui: &mut Ui,
    p: Palette,
    value: &mut bool,
    label: impl AsRef<str>,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
    let response = clickable(response);
    let changed = response.clicked();
    if changed {
        *value = !*value;
    }
    let on = ui.ctx().animate_bool_with_time(response.id, *value, 0.18);
    let hover = hover_t(ui, &response, "hover");
    let track = Rect::from_center_size(pos2(rect.left() + 20.0, rect.center().y), vec2(40.0, 22.0));
    let track_color = lerp_color(p.border_strong, p.accent, on);
    ui.painter().rect_filled(track, 11.0, track_color);
    if hover > 0.0 {
        ui.painter().rect_stroke(
            track.expand(2.0),
            13.0,
            Stroke::new(1.5, with_alpha(p.accent, 0.3 * hover)),
            StrokeKind::Outside,
        );
    }
    let knob_x = egui::lerp(track.left() + 11.0..=track.right() - 11.0, on);
    let knob = pos2(knob_x, track.center().y);
    ui.painter()
        .circle_filled(knob + vec2(0.0, 1.0), 8.5, with_alpha(Color32::BLACK, 0.18));
    ui.painter()
        .circle_filled(knob, 8.5, lerp_color(p.surface, p.accent_text, on));
    let label_x = track.right() + 12.0;
    let galley = truncated_galley(
        ui,
        label.as_ref(),
        FontId::proportional(13.0),
        p.text,
        (rect.right() - label_x).max(0.0),
    );
    ui.painter().galley(
        pos2(label_x, rect.center().y - galley.size().y * 0.5),
        galley,
        p.text,
    );
    changed
}

/// Custom checkbox matching the theme. Returns `true` on toggle.
pub(crate) fn check_box(ui: &mut Ui, p: Palette, value: &mut bool, label: impl AsRef<str>) -> bool {
    let galley = ui.painter().layout_no_wrap(
        label.as_ref().to_owned(),
        FontId::proportional(13.0),
        p.text,
    );
    let desired = vec2(18.0 + 9.0 + galley.size().x, 24.0);
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
    let response = clickable(response);
    let changed = response.clicked();
    if changed {
        *value = !*value;
    }
    let on = ui.ctx().animate_bool_with_time(response.id, *value, 0.14);
    let hover = hover_t(ui, &response, "hover");
    let box_rect =
        Rect::from_center_size(pos2(rect.left() + 9.0, rect.center().y), Vec2::splat(18.0));
    let fill = lerp_color(p.surface_elevated, p.accent, on);
    let border = lerp_color(lerp_color(p.border_strong, p.accent, hover), p.accent, on);
    ui.painter().rect_filled(box_rect, 5.0, fill);
    ui.painter()
        .rect_stroke(box_rect, 5.0, (1.0, border), StrokeKind::Inside);
    if on > 0.01 {
        let c = box_rect.center();
        let a = pos2(c.x - 4.5, c.y + 0.5);
        let b = pos2(c.x - 1.5, c.y + 3.5);
        let d = pos2(c.x + 5.0, c.y - 3.5);
        let stroke = Stroke::new(2.0, with_alpha(p.accent_text, on));
        let b_t = a.lerp(b, (on * 2.0).min(1.0));
        ui.painter().line_segment([a, b_t], stroke);
        if on > 0.5 {
            let d_t = b.lerp(d, ((on - 0.5) * 2.0).min(1.0));
            ui.painter().line_segment([b, d_t], stroke);
        }
    }
    let text_color = lerp_color(p.muted, p.text, on.max(hover));
    ui.painter().galley(
        pos2(
            box_rect.right() + 9.0,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        text_color,
    );
    changed
}

/// Segmented control with a sliding active pill.
pub(crate) fn segment_tabs(
    ui: &mut Ui,
    p: Palette,
    selected: usize,
    labels: &[impl AsRef<str>],
    scale: f32,
) -> Option<usize> {
    let mut clicked = None;
    let height = 36.0 * scale;
    let (outer, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    if !ui.is_rect_visible(outer) || labels.is_empty() {
        return None;
    }
    ui.painter().rect_filled(outer, 9.0, p.surface_elevated);
    ui.painter()
        .rect_stroke(outer, 9.0, (1.0, p.border), StrokeKind::Inside);
    let inner = outer.shrink(3.0);
    let gap = 3.0;
    let tab_w = (inner.width() - gap * (labels.len() - 1) as f32) / labels.len() as f32;
    let tab_rect = |index: f32| {
        Rect::from_min_size(
            inner.min + vec2(index * (tab_w + gap), 0.0),
            vec2(tab_w, inner.height()),
        )
    };
    let pill_index =
        ui.ctx()
            .animate_value_with_time(ui.id().with("seg_pill"), selected as f32, 0.2);
    let pill = tab_rect(pill_index);
    ui.painter().rect_filled(pill, 7.0, p.surface);
    ui.painter()
        .rect_stroke(pill, 7.0, (1.0, p.border_strong), StrokeKind::Inside);
    for (index, label) in labels.iter().enumerate() {
        let rect = tab_rect(index as f32);
        let response =
            clickable(ui.interact(rect, ui.id().with("seg_tab").with(index), Sense::click()));
        let active = (pill_index - index as f32).abs().min(1.0);
        let color = lerp_color(p.accent, p.muted, active);
        let hover = hover_t(ui, &response, "hover");
        let color = lerp_color(color, p.text, hover * active);
        let galley = ui.painter().layout_no_wrap(
            label.as_ref().to_owned(),
            FontId::proportional(12.5 * scale),
            color,
        );
        ui.painter()
            .galley(rect.center() - galley.size() * 0.5, galley, color);
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

pub(crate) fn locale_picker(
    ui: &mut Ui,
    p: Palette,
    current: Locale,
    scale: f32,
) -> Option<Locale> {
    let mut selected = None;
    egui::ComboBox::from_id_salt("header_locale")
        .selected_text(
            RichText::new(current.label())
                .size(12.0 * scale)
                .color(p.text),
        )
        .width(124.0 * scale)
        .show_ui(ui, |ui| {
            for locale in Locale::ALL {
                if clickable(ui.selectable_label(
                    current == locale,
                    RichText::new(locale.label()).size(12.0 * scale),
                ))
                .clicked()
                {
                    selected = Some(locale);
                }
            }
        });
    selected
}

// ---------------------------------------------------------------------------
// Decorations
// ---------------------------------------------------------------------------

/// Small pill with tinted background.
pub(crate) fn badge(ui: &mut Ui, text: impl AsRef<str>, color: Color32) -> Response {
    let galley =
        ui.painter()
            .layout_no_wrap(text.as_ref().to_owned(), FontId::proportional(10.5), color);
    let size = galley.size() + vec2(16.0, 7.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter()
        .rect_filled(rect, size.y * 0.5, with_alpha(color, 0.14));
    ui.painter()
        .galley(rect.center() - galley.size() * 0.5, galley, color);
    response
}

/// Status dot, optionally pulsing (requests repaint while pulsing).
pub(crate) fn status_dot(ui: &mut Ui, center: Pos2, color: Color32, pulse: bool) {
    if pulse {
        let t = ui.input(|i| i.time) as f32;
        let phase = (t * 1.6).sin() * 0.5 + 0.5;
        ui.painter().circle_filled(
            center,
            4.0 + 4.0 * phase,
            with_alpha(color, 0.25 * (1.0 - phase)),
        );
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
    ui.painter().circle_filled(center, 4.0, color);
}

/// Thin capacity bar with animated fill.
pub(crate) fn capacity_bar(ui: &mut Ui, p: Palette, ratio: f32) {
    let ratio =
        ui.ctx()
            .animate_value_with_time(ui.id().with("cap_ratio"), ratio.clamp(0.0, 1.0), 0.5);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, p.surface_elevated);
    let fill_color = if ratio > 0.9 {
        p.danger
    } else if ratio > 0.75 {
        p.warning
    } else {
        p.accent
    };
    let fill = Rect::from_min_size(
        rect.min,
        vec2(
            (rect.width() * ratio).max(if ratio > 0.0 { 6.0 } else { 0.0 }),
            rect.height(),
        ),
    );
    ui.painter().rect_filled(fill, 3.0, fill_color);
}

/// Self-drawn bell glyph (no icon font dependency).
pub(crate) fn paint_bell(painter: &egui::Painter, center: Pos2, size: f32, color: Color32) {
    let stroke = Stroke::new(size * 0.11, color);
    let r = size * 0.5;
    painter.circle_stroke(center - vec2(0.0, r * 0.15), r * 0.62, stroke);
    let left = center + vec2(-r * 0.62, -r * 0.15);
    let right = center + vec2(r * 0.62, -r * 0.15);
    painter.line_segment([left, left + vec2(0.0, r * 0.55)], stroke);
    painter.line_segment([right, right + vec2(0.0, r * 0.55)], stroke);
    painter.line_segment(
        [
            center + vec2(-r * 0.8, r * 0.4),
            center + vec2(r * 0.8, r * 0.4),
        ],
        stroke,
    );
    painter.circle_filled(center + vec2(0.0, r * 0.62), r * 0.16, color);
}

/// Chevron pointing right (breadcrumb separator).
pub(crate) fn paint_chevron(painter: &egui::Painter, center: Pos2, size: f32, color: Color32) {
    let s = size * 0.5;
    let stroke = Stroke::new(1.5, color);
    painter.line_segment(
        [center + vec2(-s * 0.4, -s), center + vec2(s * 0.4, 0.0)],
        stroke,
    );
    painter.line_segment(
        [center + vec2(s * 0.4, 0.0), center + vec2(-s * 0.4, s)],
        stroke,
    );
}

/// Small up/down sort arrow.
pub(crate) fn paint_sort_arrow(
    painter: &egui::Painter,
    center: Pos2,
    ascending: bool,
    color: Color32,
) {
    let stroke = Stroke::new(1.5, color);
    let dir = if ascending { -1.0 } else { 1.0 };
    painter.line_segment(
        [
            center + vec2(-3.5, -2.0 * dir),
            center + vec2(0.0, 2.0 * dir),
        ],
        stroke,
    );
    painter.line_segment(
        [
            center + vec2(0.0, 2.0 * dir),
            center + vec2(3.5, -2.0 * dir),
        ],
        stroke,
    );
}

pub(crate) fn vault_display_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_owned()
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// "just now" / "3m ago" / "2h ago" / "4d ago" / "2026-07-01".
pub(crate) fn relative_time(age_secs: u64) -> String {
    match age_secs {
        0..=9 => ts!("gui.time.just_now"),
        10..=59 => ts!("gui.time.seconds_ago", n = age_secs),
        60..=3599 => ts!("gui.time.minutes_ago", n = age_secs / 60),
        3600..=86_399 => ts!("gui.time.hours_ago", n = age_secs / 3600),
        _ => ts!("gui.time.days_ago", n = age_secs / 86_400),
    }
}

/// Human label for a unix timestamp: relative for the last week, else date.
pub(crate) fn timestamp_label(unix_secs: u64) -> String {
    if unix_secs == 0 {
        return "\u{2014}".to_owned();
    }
    let now = now_secs();
    let age = now.saturating_sub(unix_secs);
    if age < 7 * 86_400 {
        relative_time(age)
    } else {
        super::cache::date_label(unix_secs)
    }
}

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

pub(crate) fn kind_color(p: Palette, kind: NotificationKind) -> Color32 {
    match kind {
        NotificationKind::Success => p.success,
        NotificationKind::Error => p.danger,
        NotificationKind::Warning => p.warning,
        NotificationKind::Info => p.info,
    }
}

/// Floating toast for the latest notification, anchored bottom-right.
/// Slides up on arrival, shows a TTL progress line, fades out at the end.
pub(crate) fn show_toast(ui: &mut Ui, app: &mut App) {
    let Some(latest) = app.notifications.last() else {
        return;
    };
    let age = latest.at.elapsed();
    if age >= super::app::TOAST_TTL {
        return;
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(40));
    let ttl = super::app::TOAST_TTL.as_secs_f32();
    let age_s = age.as_secs_f32();
    let enter = ease_out(age_s / 0.28);
    let fade_start = ttl - super::app::TOAST_FADE.as_secs_f32();
    let exit = if age_s > fade_start {
        1.0 - (age_s - fade_start) / super::app::TOAST_FADE.as_secs_f32()
    } else {
        1.0
    };
    let opacity = enter.min(exit).clamp(0.0, 1.0);

    let p = app.palette;
    let tone = kind_color(p, latest.kind);
    let kind = latest.kind;
    let text = strip_status_glyph(&latest.text).to_owned();
    let progress = 1.0 - age_s / ttl;

    let screen = ui.ctx().content_rect();
    let width = (screen.width() * 0.5).clamp(260.0, 420.0);
    let slide = (1.0 - enter) * 18.0;
    let anchor = pos2(screen.right() - 20.0, screen.bottom() - 20.0 + slide);

    egui::Area::new(egui::Id::new("closed_sesame_toast"))
        .order(egui::Order::Foreground)
        .pivot(Align2::RIGHT_BOTTOM)
        .fixed_pos(anchor)
        .interactable(true)
        .show(ui.ctx(), |ui| {
            ui.set_opacity(opacity);
            ui.set_max_width(width);
            Frame::new()
                .fill(p.surface)
                .stroke((1.0, p.border_strong))
                .corner_radius(12.0)
                .inner_margin(egui::Margin {
                    left: 16,
                    right: 12,
                    top: 12,
                    bottom: 12,
                })
                .shadow(egui::epaint::Shadow {
                    offset: [0, 10],
                    blur: 26,
                    spread: 0,
                    color: p.shadow,
                })
                .show(ui, |ui| {
                    ui.set_width(width - 28.0);
                    let frame_rect = ui.max_rect().expand2(vec2(16.0, 12.0));
                    ui.horizontal_top(|ui| {
                        let (dot_rect, _) =
                            ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                        paint_kind_glyph(ui.painter(), dot_rect.center(), kind, tone);
                        ui.add_space(4.0);
                        ui.vertical(|ui| {
                            ui.set_width(width - 28.0 - 18.0 - 4.0 - 30.0);
                            let mut lines = text.lines();
                            if let Some(first) = lines.next() {
                                ui.add(
                                    egui::Label::new(RichText::new(first).size(13.0).color(p.text))
                                        .wrap(),
                                );
                            }
                            let rest: Vec<&str> = lines.take(4).collect();
                            if !rest.is_empty() {
                                ui.add_space(2.0);
                                for line in rest {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(line).size(11.5).color(p.muted),
                                        )
                                        .wrap(),
                                    );
                                }
                            }
                        });
                        ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                            if flat_icon_button(ui, p, Icon::X, 24.0).clicked() {
                                app.dismiss_toast();
                            }
                        });
                    });
                    // TTL progress line along the bottom edge.
                    let line = Rect::from_min_size(
                        pos2(frame_rect.left() + 10.0, frame_rect.bottom() - 2.5),
                        vec2((frame_rect.width() - 20.0) * progress.clamp(0.0, 1.0), 2.0),
                    );
                    ui.painter().rect_filled(line, 1.0, with_alpha(tone, 0.8));
                });
        });
}

/// Check / cross / bang / dot glyph for a notification kind.
pub(crate) fn paint_kind_glyph(
    painter: &egui::Painter,
    center: Pos2,
    kind: NotificationKind,
    color: Color32,
) {
    painter.circle_filled(center, 9.0, with_alpha(color, 0.16));
    let stroke = Stroke::new(1.8, color);
    match kind {
        NotificationKind::Success => {
            painter.line_segment([center + vec2(-4.0, 0.0), center + vec2(-1.0, 3.0)], stroke);
            painter.line_segment([center + vec2(-1.0, 3.0), center + vec2(4.5, -3.0)], stroke);
        }
        NotificationKind::Error => {
            painter.line_segment([center + vec2(-3.5, -3.5), center + vec2(3.5, 3.5)], stroke);
            painter.line_segment([center + vec2(3.5, -3.5), center + vec2(-3.5, 3.5)], stroke);
        }
        NotificationKind::Warning => {
            painter.line_segment([center + vec2(0.0, -4.0), center + vec2(0.0, 1.0)], stroke);
            painter.circle_filled(center + vec2(0.0, 3.8), 1.2, color);
        }
        NotificationKind::Info => {
            painter.circle_filled(center + vec2(0.0, -3.6), 1.2, color);
            painter.line_segment([center + vec2(0.0, -1.0), center + vec2(0.0, 4.0)], stroke);
        }
    }
}

/// Strip the leading status glyph (check / cross / warning sign) that drives
/// notification kind detection; the UI renders the kind as an icon instead.
pub(crate) fn strip_status_glyph(text: &str) -> &str {
    text.trim_start_matches(['\u{2713}', '\u{2717}', '\u{26A0}'])
        .trim_start()
}
