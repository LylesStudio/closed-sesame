//! Responsive layout primitives.
//!
//! There is no fixed design canvas: every view fills the viewport natively
//! via egui panels, so any window size shows complete content (no
//! letterboxing, no clipping). Window size itself is constrained by the
//! viewport's `min_inner_size` below, and hero-style pages scale their
//! decorative elements with [`content_scale`].

use egui::{Color32, Ui};

/// Initial window size.
pub(crate) const DESIGN_WIDTH: f32 = 960.0;
pub(crate) const DESIGN_HEIGHT: f32 = 640.0;

/// Minimum window size: below this, content would become unusably cramped,
/// so the WM is not allowed to shrink the window further.
pub(crate) const MIN_WINDOW_WIDTH: f32 = 560.0;
pub(crate) const MIN_WINDOW_HEIGHT: f32 = 420.0;

/// Fill the whole viewport background with the theme color.
pub(crate) fn fill_background(ui: &mut Ui, bg: Color32) {
    ui.painter().rect_filled(ui.max_rect(), 0.0, bg);
}

/// Mild scale factor for decorative/hero elements on form pages, derived
/// from the viewport height relative to the reference design (640pt).
/// Content layout itself always adapts to the real viewport size; this only
/// tunes logo/wordmark sizing so small windows get a compact hero and large
/// windows a slightly roomier one.
pub(crate) fn content_scale(ui: &Ui) -> f32 {
    (ui.max_rect().height() / DESIGN_HEIGHT).clamp(0.8, 1.3)
}
