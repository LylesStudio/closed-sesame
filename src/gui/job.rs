//! Background container work with a progress readout.
//!
//! Batch adds, extracts, copies and maintenance run off the UI thread so the
//! window can paint a progress bar. The worker reports counts through a shared
//! snapshot; the UI thread only applies the result.

use super::theme::{Palette, with_alpha};
use crate::error::SesameError;
use egui::{Align2, Color32, Rect, RichText, Sense, Ui, vec2};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use super::actions::BatchAddResult;

#[derive(Clone)]
pub(crate) struct Progress {
    inner: Arc<Mutex<Snapshot>>,
}

struct Snapshot {
    completed: u64,
    total: u64,
    detail: String,
}

impl Progress {
    fn new(total: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Snapshot {
                completed: 0,
                total,
                detail: String::new(),
            })),
        }
    }

    pub(crate) fn report(&self, completed: u64, total: u64, detail: impl Into<String>) {
        if let Ok(mut snapshot) = self.inner.lock() {
            snapshot.completed = completed;
            snapshot.total = total;
            snapshot.detail = detail.into();
        }
    }

    fn get(&self) -> (u64, u64, String) {
        self.inner
            .lock()
            .map(|snapshot| (snapshot.completed, snapshot.total, snapshot.detail.clone()))
            .unwrap_or((0, 0, String::new()))
    }
}

pub(crate) enum JobOutput {
    Add(BatchAddResult),
    Extracted {
        count: usize,
        removed: Vec<crate::FileEntry>,
    },
    Deleted(Vec<crate::FileEntry>),
    Verified(u64),
    Defrag {
        files: usize,
    },
    Recovered {
        extracted: usize,
        failed: usize,
        skipped: usize,
    },
    Transferred {
        count: usize,
        copied: bool,
    },
    #[cfg(feature = "mount")]
    Mounted {
        mounted: crate::mount::MountedVault,
        container: std::path::PathBuf,
    },
}

pub(crate) struct ActiveJob {
    pub(crate) title: String,
    progress: Progress,
    rx: Receiver<Result<JobOutput, SesameError>>,
}

impl ActiveJob {
    pub(crate) fn spawn(
        title: impl Into<String>,
        total: u64,
        work: impl FnOnce(Progress) -> Result<JobOutput, SesameError> + Send + 'static,
    ) -> Self {
        let progress = Progress::new(total);
        let worker = progress.clone();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(work(worker));
        });
        Self {
            title: title.into(),
            progress,
            rx,
        }
    }

    pub(crate) fn poll(&self) -> Option<Result<JobOutput, SesameError>> {
        self.rx.try_recv().ok()
    }
}

pub(crate) fn overlay(ui: &mut Ui, job: &ActiveJob, p: Palette) {
    let (completed, total, detail) = job.progress.get();
    let screen = ui.ctx().content_rect();
    egui::Area::new(egui::Id::new("job_backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .interactable(true)
        .show(ui.ctx(), |ui| {
            let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(
                rect,
                0.0,
                with_alpha(
                    if p.is_dark() {
                        Color32::BLACK
                    } else {
                        Color32::from_rgb(30, 30, 34)
                    },
                    0.45,
                ),
            );
        });

    egui::Area::new(egui::Id::new("job_card"))
        .order(egui::Order::Foreground)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .interactable(true)
        .show(ui.ctx(), |ui| {
            let width = 420.0_f32.min(screen.width() * 0.9);
            super::widgets::card_frame(p).show(ui, |ui| {
                ui.set_width(width);
                ui.label(RichText::new(&job.title).size(16.0).strong().color(p.text));
                ui.add_space(8.0);
                if !detail.is_empty() {
                    ui.add(
                        egui::Label::new(RichText::new(&detail).size(12.0).color(p.muted))
                            .truncate(),
                    );
                    ui.add_space(10.0);
                }
                let (bar, _) = ui.allocate_exact_size(vec2(width, 8.0), Sense::hover());
                ui.painter().rect_filled(bar, 4.0, p.surface_elevated);
                let fraction = if total == 0 {
                    let t = ui.input(|input| input.time) as f32;
                    let travel = (t * 0.35).fract();
                    let head = bar.left() + bar.width() * travel;
                    let span = bar.width() * 0.35;
                    let fill = Rect::from_min_max(
                        egui::pos2(head, bar.top()),
                        egui::pos2((head + span).min(bar.right()), bar.bottom()),
                    );
                    ui.painter().rect_filled(fill, 4.0, p.accent);
                    travel
                } else {
                    let fraction = (completed as f32 / total as f32).clamp(0.0, 1.0);
                    let fill = Rect::from_min_max(
                        bar.min,
                        egui::pos2(bar.left() + bar.width() * fraction, bar.bottom()),
                    );
                    ui.painter().rect_filled(fill, 4.0, p.accent);
                    fraction
                };
                ui.add_space(8.0);
                let label = if total == 0 {
                    crate::ts!("gui.job.working")
                } else {
                    crate::ts!(
                        "gui.job.progress",
                        done = completed,
                        total = total,
                        percent = (fraction * 100.0).round() as u32
                    )
                };
                ui.label(RichText::new(label).size(12.0).color(p.muted));
            });
        });
}
