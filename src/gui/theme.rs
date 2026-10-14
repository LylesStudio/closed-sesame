//! Closed Sesame visual themes.
//!
//! Two palettes tied to the brand:
//! - **Cave** (dark): obsidian black with antique gold — the sealed vault.
//! - **Parchment** (light): gray-white with brushed silver — the opened scroll.
//!
//! [`Theme`] is the user's choice; [`Palette`] is the concrete set of colors
//! painted this frame. Keeping them apart lets the app cross-fade between
//! themes: every frame blends the two palettes by an animated factor, so a
//! theme switch glides instead of snapping.

use egui::{Color32, CornerRadius, FontId, Margin, Pos2, Rect, Style, TextStyle, Visuals, vec2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Cave,
    Parchment,
}

impl Theme {
    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::Cave => Self::Parchment,
            Self::Parchment => Self::Cave,
        }
    }

    pub(crate) fn label(self) -> String {
        match self {
            Self::Cave => ts!("theme.cave"),
            Self::Parchment => ts!("theme.parchment"),
        }
    }

    /// Blend factor used by the cross-fade: 0 = Cave, 1 = Parchment.
    pub(crate) fn blend_target(self) -> f32 {
        match self {
            Self::Cave => 0.0,
            Self::Parchment => 1.0,
        }
    }

    pub(crate) fn palette(self) -> Palette {
        match self {
            Self::Cave => Palette::CAVE,
            Self::Parchment => Palette::PARCHMENT,
        }
    }
}

/// Concrete colors for one frame. All fields are plain colors so the whole
/// struct can be linearly blended between the two themes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Palette {
    /// Window background.
    pub(crate) bg: Color32,
    /// Slightly lifted background used behind the hero / sidebar.
    pub(crate) bg_raised: Color32,
    /// Card / panel background.
    pub(crate) surface: Color32,
    /// Raised elements: inputs, hovered rows, chips.
    pub(crate) surface_elevated: Color32,
    pub(crate) border: Color32,
    pub(crate) border_strong: Color32,
    pub(crate) text: Color32,
    pub(crate) muted: Color32,
    pub(crate) faint: Color32,
    /// Brand accent: antique gold in the dark vault, graphite silver in daylight.
    pub(crate) accent: Color32,
    /// Translucent accent wash for badges, selection, focus rings.
    pub(crate) accent_soft: Color32,
    /// Diffuse accent glow used behind hero elements.
    pub(crate) accent_glow: Color32,
    /// Fill of primary buttons.
    pub(crate) primary_fill: Color32,
    /// Text/icons on top of [`primary_fill`](Self::primary_fill).
    pub(crate) accent_text: Color32,
    pub(crate) danger: Color32,
    pub(crate) danger_soft: Color32,
    pub(crate) success: Color32,
    pub(crate) warning: Color32,
    pub(crate) info: Color32,
    pub(crate) shadow: Color32,
    /// 0 = fully dark theme, 1 = fully light theme (for mixed decisions).
    pub(crate) lightness: f32,
}

impl Palette {
    pub(crate) const CAVE: Self = Self {
        bg: Color32::from_rgb(6, 6, 8),
        bg_raised: Color32::from_rgb(10, 10, 13),
        surface: Color32::from_rgb(15, 15, 18),
        surface_elevated: Color32::from_rgb(26, 27, 31),
        border: Color32::from_rgb(38, 40, 45),
        border_strong: Color32::from_rgb(62, 65, 72),
        text: Color32::from_rgb(240, 239, 234),
        muted: Color32::from_rgb(142, 144, 150),
        faint: Color32::from_rgb(86, 88, 94),
        accent: Color32::from_rgb(212, 175, 55),
        accent_soft: Color32::from_rgba_premultiplied(34, 28, 9, 255),
        accent_glow: Color32::from_rgba_premultiplied(42, 34, 10, 140),
        primary_fill: Color32::from_rgb(212, 175, 55),
        accent_text: Color32::from_rgb(14, 12, 4),
        danger: Color32::from_rgb(239, 88, 88),
        danger_soft: Color32::from_rgba_premultiplied(48, 18, 18, 255),
        success: Color32::from_rgb(74, 199, 134),
        warning: Color32::from_rgb(230, 170, 60),
        info: Color32::from_rgb(120, 160, 230),
        shadow: Color32::from_black_alpha(110),
        lightness: 0.0,
    };

    pub(crate) const PARCHMENT: Self = Self {
        bg: Color32::from_rgb(243, 241, 236),
        bg_raised: Color32::from_rgb(238, 235, 228),
        surface: Color32::from_rgb(255, 255, 255),
        surface_elevated: Color32::from_rgb(236, 233, 226),
        border: Color32::from_rgb(220, 216, 207),
        border_strong: Color32::from_rgb(194, 189, 178),
        text: Color32::from_rgb(30, 28, 25),
        muted: Color32::from_rgb(122, 117, 108),
        faint: Color32::from_rgb(172, 167, 158),
        // Aged brass: the same metal as the dark theme, seen in daylight.
        accent: Color32::from_rgb(150, 108, 24),
        accent_soft: Color32::from_rgba_premultiplied(244, 236, 218, 255),
        accent_glow: Color32::from_rgba_premultiplied(100, 93, 74, 110),
        primary_fill: Color32::from_rgb(46, 41, 34),
        accent_text: Color32::from_rgb(246, 242, 232),
        danger: Color32::from_rgb(200, 44, 44),
        danger_soft: Color32::from_rgba_premultiplied(250, 226, 226, 255),
        success: Color32::from_rgb(22, 138, 74),
        warning: Color32::from_rgb(190, 130, 20),
        info: Color32::from_rgb(60, 100, 190),
        shadow: Color32::from_black_alpha(40),
        lightness: 1.0,
    };

    pub(crate) fn is_dark(&self) -> bool {
        self.lightness < 0.5
    }

    /// Linear blend between two palettes.
    pub(crate) fn blend(a: &Self, b: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let c = |x: Color32, y: Color32| lerp_color(x, y, t);
        Self {
            bg: c(a.bg, b.bg),
            bg_raised: c(a.bg_raised, b.bg_raised),
            surface: c(a.surface, b.surface),
            surface_elevated: c(a.surface_elevated, b.surface_elevated),
            border: c(a.border, b.border),
            border_strong: c(a.border_strong, b.border_strong),
            text: c(a.text, b.text),
            muted: c(a.muted, b.muted),
            faint: c(a.faint, b.faint),
            accent: c(a.accent, b.accent),
            accent_soft: c(a.accent_soft, b.accent_soft),
            accent_glow: c(a.accent_glow, b.accent_glow),
            primary_fill: c(a.primary_fill, b.primary_fill),
            accent_text: c(a.accent_text, b.accent_text),
            danger: c(a.danger, b.danger),
            danger_soft: c(a.danger_soft, b.danger_soft),
            success: c(a.success, b.success),
            warning: c(a.warning, b.warning),
            info: c(a.info, b.info),
            shadow: c(a.shadow, b.shadow),
            lightness: a.lightness + (b.lightness - a.lightness) * t,
        }
    }

    pub(crate) fn card_radius(&self) -> f32 {
        14.0
    }

    pub(crate) fn sidebar_width(&self) -> f32 {
        264.0
    }

    /// Push this palette into egui's global style so built-in widgets
    /// (scroll bars, combo boxes, sliders, context menus) match the design.
    pub(crate) fn apply(&self, ctx: &egui::Context) {
        let mut visuals = if self.is_dark() {
            Visuals::dark()
        } else {
            Visuals::light()
        };

        let radius = CornerRadius::same(8);
        visuals.override_text_color = Some(self.text);
        visuals.panel_fill = self.bg;
        visuals.window_fill = self.surface;
        visuals.faint_bg_color = self.surface;
        visuals.extreme_bg_color = self.surface_elevated;
        visuals.window_corner_radius = CornerRadius::same(12);
        visuals.window_stroke.color = self.border;
        visuals.window_stroke.width = 1.0;
        visuals.window_shadow = egui::epaint::Shadow {
            offset: [0, 10],
            blur: 28,
            spread: 0,
            color: self.shadow,
        };
        visuals.popup_shadow = visuals.window_shadow;
        visuals.menu_corner_radius = CornerRadius::same(10);

        visuals.widgets.noninteractive.bg_fill = self.surface;
        visuals.widgets.noninteractive.weak_bg_fill = self.surface;
        visuals.widgets.noninteractive.bg_stroke.color = self.border;
        visuals.widgets.noninteractive.fg_stroke.color = self.text;
        visuals.widgets.noninteractive.corner_radius = radius;

        visuals.widgets.inactive.bg_fill = self.surface_elevated;
        visuals.widgets.inactive.weak_bg_fill = self.surface_elevated;
        visuals.widgets.inactive.bg_stroke.color = self.border;
        visuals.widgets.inactive.fg_stroke.color = self.text;
        visuals.widgets.inactive.corner_radius = radius;

        visuals.widgets.hovered.bg_fill = self.surface_elevated;
        visuals.widgets.hovered.weak_bg_fill = self.surface_elevated;
        visuals.widgets.hovered.bg_stroke.color = self.border_strong;
        visuals.widgets.hovered.fg_stroke.color = self.text;
        visuals.widgets.hovered.corner_radius = radius;

        visuals.widgets.active.bg_fill = self.accent_soft;
        visuals.widgets.active.weak_bg_fill = self.accent_soft;
        visuals.widgets.active.bg_stroke.color = self.accent;
        visuals.widgets.active.fg_stroke.color = self.text;
        visuals.widgets.active.corner_radius = radius;

        visuals.widgets.open.bg_fill = self.surface_elevated;
        visuals.widgets.open.weak_bg_fill = self.surface_elevated;
        visuals.widgets.open.bg_stroke.color = self.border_strong;
        visuals.widgets.open.fg_stroke.color = self.text;
        visuals.widgets.open.corner_radius = radius;

        visuals.selection.bg_fill = self.accent_soft;
        visuals.selection.stroke.color = self.accent;
        visuals.hyperlink_color = self.accent;
        visuals.text_cursor.stroke.color = self.accent;
        visuals.slider_trailing_fill = true;

        let mut style = Style {
            visuals,
            ..Style::default()
        };
        style.spacing.item_spacing = vec2(10.0, 10.0);
        style.spacing.button_padding = vec2(14.0, 8.0);
        style.spacing.window_margin = Margin::same(18);
        style.spacing.menu_margin = Margin::same(8);
        style.spacing.indent = 18.0;
        style.spacing.interact_size = vec2(40.0, 30.0);
        style.spacing.scroll.bar_width = 6.0;
        style.spacing.scroll.floating = true;
        style.spacing.slider_width = 160.0;
        style.spacing.combo_width = 140.0;
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(13.5));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(11.0));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(20.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(12.5));
        style.animation_time = 0.14;

        ctx.set_global_style(style);
    }
}

/// Gamma-space linear interpolation between two colors.
pub(crate) fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    egui::lerp(egui::Rgba::from(a)..=egui::Rgba::from(b), t).into()
}

/// Color with its alpha scaled (works on premultiplied colors).
pub(crate) fn with_alpha(color: Color32, alpha: f32) -> Color32 {
    color.gamma_multiply(alpha.clamp(0.0, 1.0))
}

/// Vertical two-stop gradient fill (no rounding; use inside a clipped frame).
pub(crate) fn paint_vertical_gradient(
    painter: &egui::Painter,
    rect: Rect,
    top: Color32,
    bottom: Color32,
) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// Soft radial glow: a triangle fan from `center` (full color) fading to
/// transparent at `radius`. `falloff` > 1 tightens the core.
pub(crate) fn paint_radial_glow(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    color: Color32,
    falloff: f32,
) {
    if radius <= 0.0 || color.a() == 0 {
        return;
    }
    const RINGS: usize = 16;
    const SEGMENTS: usize = 48;
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(center, color);
    for ring in 1..=RINGS {
        let f = ring as f32 / RINGS as f32;
        let alpha = (1.0 - f).powf(falloff);
        let ring_color = with_alpha(color, alpha);
        let r = radius * f;
        for seg in 0..SEGMENTS {
            let angle = seg as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(
                Pos2::new(center.x + r * angle.cos(), center.y + r * angle.sin()),
                ring_color,
            );
        }
    }
    let idx = |ring: usize, seg: usize| -> u32 {
        if ring == 0 {
            0
        } else {
            (1 + (ring - 1) * SEGMENTS + (seg % SEGMENTS)) as u32
        }
    };
    for ring in 1..=RINGS {
        for seg in 0..SEGMENTS {
            let a = idx(ring, seg);
            let b = idx(ring, seg + 1);
            if ring == 1 {
                mesh.add_triangle(0, a, b);
            } else {
                let c = idx(ring - 1, seg);
                let d = idx(ring - 1, seg + 1);
                mesh.add_triangle(c, a, b);
                mesh.add_triangle(c, b, d);
            }
        }
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// Smoothstep easing for hand-rolled animations.
pub(crate) fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_endpoints_are_exact() {
        let a = Palette::CAVE;
        let b = Palette::PARCHMENT;
        assert_eq!(Palette::blend(&a, &b, 0.0), a);
        assert_eq!(Palette::blend(&a, &b, 1.0), b);
        let mid = Palette::blend(&a, &b, 0.5);
        assert!(mid.lightness > 0.4 && mid.lightness < 0.6);
    }

    #[test]
    fn theme_serializes_kebab_case() {
        assert_eq!(
            serde_json::to_string(&Theme::Parchment).unwrap(),
            "\"parchment\""
        );
        assert_eq!(
            serde_json::from_str::<Theme>("\"cave\"").unwrap(),
            Theme::Cave
        );
    }
}
