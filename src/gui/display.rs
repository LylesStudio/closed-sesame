//! Keep the main window usable when it crosses onto another monitor.
//!
//! A DPI change should leave the window either the same logical size or the
//! same physical size. Anything else (the size multiplied by the scale ratio
//! a second time, or a window larger than the monitor it just entered) is
//! pulled back. Maximized windows are left to the OS.

use egui::Vec2;

const MIN_LOGICAL: f32 = 64.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Sample {
    pub(crate) ppp: f32,
    pub(crate) logical: Vec2,
    pub(crate) monitor: Vec2,
    pub(crate) maximized: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Action {
    None,
    /// The monitor or its scale changed; content protection should be sent again.
    ReapplyCapture,
    /// Logical size to apply. Also implies [`Action::ReapplyCapture`].
    Refit(Vec2),
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Stability {
    baseline: Option<Sample>,
    /// Frames to ignore after we ask the OS to resize, so the in-between
    /// size is not treated as a new user choice.
    settle: u8,
}

impl Stability {
    pub(crate) fn observe(&mut self, sample: Option<Sample>) -> Action {
        let Some(sample) = sample else {
            return Action::None;
        };
        if !usable_scale(sample.ppp) || !usable_size(sample.logical) {
            return Action::None;
        }

        let Some(prev) = self.baseline else {
            self.baseline = Some(sample);
            return Action::None;
        };

        if self.settle > 0 {
            self.settle -= 1;
            self.baseline = Some(Sample {
                logical: prev.logical,
                ..sample
            });
            return Action::None;
        }

        let switched = switched_monitor(prev, sample);
        if sample.maximized {
            self.baseline = Some(sample);
            return if switched {
                Action::ReapplyCapture
            } else {
                Action::None
            };
        }

        if (sample.ppp - prev.ppp).abs() > 0.05 {
            let keep_logical = prev.logical;
            let keep_physical = prev.logical * (prev.ppp / sample.ppp);
            if !near(sample.logical, keep_logical) && !near(sample.logical, keep_physical) {
                return self.refit(fit_monitor(keep_logical, sample.monitor), sample);
            }
        }

        if let Some(fitted) = overflow(sample.logical, sample.monitor) {
            return self.refit(fitted, sample);
        }

        self.baseline = Some(sample);
        if switched {
            Action::ReapplyCapture
        } else {
            Action::None
        }
    }

    fn refit(&mut self, size: Vec2, sample: Sample) -> Action {
        let size = Vec2::new(size.x.max(MIN_LOGICAL), size.y.max(MIN_LOGICAL));
        self.baseline = Some(Sample {
            logical: size,
            ..sample
        });
        self.settle = 3;
        Action::Refit(size)
    }
}

fn switched_monitor(prev: Sample, sample: Sample) -> bool {
    (sample.ppp - prev.ppp).abs() > 0.05 || (sample.monitor - prev.monitor).length_sq() > 4.0
}

fn usable_scale(ppp: f32) -> bool {
    ppp.is_finite() && ppp > 0.1
}

fn usable_size(size: Vec2) -> bool {
    size.x.is_finite() && size.y.is_finite() && size.x >= 32.0 && size.y >= 32.0
}

fn near(a: Vec2, b: Vec2) -> bool {
    if a.x < 1.0 || a.y < 1.0 || b.x < 1.0 || b.y < 1.0 {
        return false;
    }
    ((a.x / b.x) - 1.0).abs() < 0.12 && ((a.y / b.y) - 1.0).abs() < 0.12
}

/// Shrink only when the window is larger than the monitor it is on.
fn overflow(size: Vec2, monitor: Vec2) -> Option<Vec2> {
    if monitor.x < 100.0 || monitor.y < 100.0 {
        return None;
    }
    let cap = Vec2::new(
        (monitor.x - 8.0).max(MIN_LOGICAL),
        (monitor.y - 8.0).max(MIN_LOGICAL),
    );
    if size.x <= cap.x + 1.0 && size.y <= cap.y + 1.0 {
        return None;
    }
    Some(Vec2::new(size.x.min(cap.x), size.y.min(cap.y)))
}

fn fit_monitor(size: Vec2, monitor: Vec2) -> Vec2 {
    overflow(size, monitor).unwrap_or(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(ppp: f32, w: f32, h: f32, mw: f32, mh: f32) -> Sample {
        Sample {
            ppp,
            logical: Vec2::new(w, h),
            monitor: Vec2::new(mw, mh),
            maximized: false,
        }
    }

    #[test]
    fn the_first_sample_is_only_recorded() {
        let mut stability = Stability::default();
        assert_eq!(
            stability.observe(Some(sample(1.0, 960.0, 640.0, 1920.0, 1080.0))),
            Action::None
        );
    }

    #[test]
    fn a_second_scale_application_is_pulled_back_to_the_logical_size() {
        let mut stability = Stability::default();
        stability.observe(Some(sample(1.0, 960.0, 640.0, 1920.0, 1080.0)));
        // 960 * 1.5 is neither the old logical size nor the physical-preserving one.
        assert_eq!(
            stability.observe(Some(sample(1.5, 1440.0, 960.0, 1707.0, 960.0))),
            Action::Refit(Vec2::new(960.0, 640.0))
        );
    }

    #[test]
    fn keeping_the_physical_size_is_left_alone() {
        let mut stability = Stability::default();
        stability.observe(Some(sample(1.0, 960.0, 640.0, 1920.0, 1080.0)));
        assert_eq!(
            stability.observe(Some(sample(1.5, 640.0, 426.7, 1707.0, 960.0))),
            Action::ReapplyCapture
        );
    }

    #[test]
    fn a_window_larger_than_the_new_monitor_is_fitted() {
        let mut stability = Stability::default();
        stability.observe(Some(sample(1.0, 1600.0, 1000.0, 1920.0, 1080.0)));
        assert_eq!(
            stability.observe(Some(sample(1.0, 1600.0, 1000.0, 1280.0, 800.0))),
            Action::Refit(Vec2::new(1272.0, 792.0))
        );
    }

    #[test]
    fn a_maximized_window_is_not_resized() {
        let mut stability = Stability::default();
        stability.observe(Some(sample(1.0, 1920.0, 1080.0, 1920.0, 1080.0)));
        let mut next = sample(1.25, 2048.0, 1152.0, 2048.0, 1152.0);
        next.maximized = true;
        assert_eq!(stability.observe(Some(next)), Action::ReapplyCapture);
    }

    #[test]
    fn a_minimized_window_does_not_replace_the_baseline() {
        let mut stability = Stability::default();
        stability.observe(Some(sample(1.0, 960.0, 640.0, 1920.0, 1080.0)));
        assert_eq!(
            stability.observe(Some(sample(1.0, 0.0, 0.0, 1920.0, 1080.0))),
            Action::None
        );
        assert_eq!(
            stability.observe(Some(sample(1.5, 1440.0, 960.0, 1707.0, 960.0))),
            Action::Refit(Vec2::new(960.0, 640.0))
        );
    }
}
