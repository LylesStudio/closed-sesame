//! Modal confirmation and info dialogs.
//!
//! The backdrop fades in and the card scales up from 96% so the modal feels
//! like it arrives rather than pops. Escape cancels, Enter confirms where it
//! is unambiguous.

use super::app::App;
use super::cache::ConflictResolution;
use super::icons::{Icon, paint_icon};
use super::theme::{Palette, with_alpha};
use super::widgets::{
    card_frame, check_box, ghost_button, input_field, primary_button_sized, solid_button,
};
use egui::{Align, Align2, Color32, Layout, Rect, RichText, Sense, Ui, Vec2, vec2};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub(crate) enum ActiveDialog {
    DeleteConfirm {
        paths: Vec<String>,
    },
    ExtractInfo {
        paths: Vec<String>,
        dont_show_again: bool,
    },
    SymlinkInfo {
        paths: Vec<PathBuf>,
        dont_show_again: bool,
    },
    UnlockRecent {
        path: String,
        password: String,
        no_password: bool,
    },
    /// A cache file was edited while the vault entry also changed.
    WritebackConflict {
        container_path: String,
    },
    /// Extract, then delete the vault entries. Always confirmed.
    MoveOut {
        paths: Vec<String>,
    },
    /// Copy or move the selection to a directory inside the vault.
    Transfer {
        copy: bool,
        paths: Vec<String>,
        destination: String,
    },
    ChangePassword {
        password: String,
        confirm: String,
    },
    Grow {
        size: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DialogOutcome {
    DeleteConfirm {
        paths: Vec<String>,
    },
    ExtractProceed {
        paths: Vec<String>,
        skip_future: bool,
    },
    SymlinkProceed {
        paths: Vec<PathBuf>,
        skip_future: bool,
    },
    UnlockRecent {
        password: String,
        no_password: bool,
    },
    WritebackResolve {
        container_path: String,
        resolution: ConflictResolution,
    },
    MoveOut {
        paths: Vec<String>,
    },
    Transfer {
        copy: bool,
        paths: Vec<String>,
        destination: String,
    },
    ChangePassword {
        password: String,
        confirm: String,
    },
    Grow {
        size: String,
    },
    Cancelled,
}

pub(crate) fn show(app: &mut App, ui: &mut Ui) -> Option<DialogOutcome> {
    let open_t =
        ui.ctx()
            .animate_bool_with_time(egui::Id::new("modal_open"), app.dialog.is_some(), 0.18);
    let p = app.palette;
    let dialog = app.dialog.as_mut()?;

    let screen = ui.ctx().content_rect();
    // Backdrop: swallow clicks and dim the page.
    egui::Area::new(egui::Id::new("closed_sesame_modal_backdrop"))
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
                        egui::Color32::BLACK
                    } else {
                        egui::Color32::from_rgb(30, 30, 34)
                    },
                    0.55 * open_t,
                ),
            );
        });

    let scale = (screen.width() / 960.0)
        .min(screen.height() / 640.0)
        .clamp(0.8, 1.2);
    let card_width = (440.0 * scale).min(screen.width() * 0.92);
    let mut outcome = None;
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));

    egui::Area::new(egui::Id::new("closed_sesame_modal"))
        .order(egui::Order::Foreground)
        .anchor(Align2::CENTER_CENTER, vec2(0.0, (1.0 - open_t) * 12.0))
        .interactable(true)
        .show(ui.ctx(), |ui| {
            ui.set_opacity(open_t);
            ui.set_max_width(card_width);
            card_frame(p).show(ui, |ui| {
                ui.set_width(card_width - 40.0);
                match dialog {
                    ActiveDialog::DeleteConfirm { paths } => {
                        header(
                            ui,
                            p,
                            Icon::Trash,
                            p.danger,
                            crate::ts!("gui.dialog.delete_title"),
                        );
                        let body_text = if paths.len() > 1 {
                            crate::ts!("gui.dialog.delete_batch_body", count = paths.len())
                        } else {
                            crate::ts!("gui.dialog.delete_body")
                        };
                        body(ui, p, body_text);
                        ui.add_space(10.0);
                        path_list(ui, p, paths);
                        ui.add_space(18.0);
                        let confirm = crate::ts!("gui.browser.delete");
                        outcome = confirm_row(ui, p, |ui| {
                            if solid_button(ui, confirm, p.danger, egui::Color32::WHITE, 110.0)
                                .clicked()
                            {
                                return Some(DialogOutcome::DeleteConfirm {
                                    paths: paths.clone(),
                                });
                            }
                            None
                        });
                    }
                    ActiveDialog::ExtractInfo {
                        paths,
                        dont_show_again,
                    } => {
                        header(
                            ui,
                            p,
                            Icon::Download,
                            p.accent,
                            crate::ts!("gui.dialog.extract_title"),
                        );
                        body(ui, p, crate::ts!("gui.dialog.extract_body"));
                        if paths.len() > 1 {
                            ui.add_space(8.0);
                            muted(
                                ui,
                                p,
                                &crate::ts!("gui.browser.selection_count", count = paths.len()),
                            );
                        }
                        ui.add_space(14.0);
                        check_box(
                            ui,
                            p,
                            dont_show_again,
                            crate::ts!("gui.dialog.dont_show_again"),
                        );
                        ui.add_space(14.0);
                        let skip = *dont_show_again;
                        outcome = confirm_row(ui, p, |ui| {
                            if primary_button_sized(
                                ui,
                                p,
                                None,
                                crate::ts!("gui.dialog.confirm"),
                                120.0,
                                36.0,
                                0.95,
                            )
                            .clicked()
                            {
                                return Some(DialogOutcome::ExtractProceed {
                                    paths: paths.clone(),
                                    skip_future: skip,
                                });
                            }
                            None
                        });
                    }
                    ActiveDialog::SymlinkInfo {
                        paths,
                        dont_show_again,
                    } => {
                        header(
                            ui,
                            p,
                            Icon::ExternalLink,
                            p.accent,
                            crate::ts!("gui.dialog.symlink_title"),
                        );
                        body(ui, p, crate::ts!("gui.dialog.symlink_body"));
                        ui.add_space(14.0);
                        check_box(
                            ui,
                            p,
                            dont_show_again,
                            crate::ts!("gui.dialog.dont_show_again"),
                        );
                        ui.add_space(14.0);
                        let skip = *dont_show_again;
                        outcome = confirm_row(ui, p, |ui| {
                            if primary_button_sized(
                                ui,
                                p,
                                None,
                                crate::ts!("gui.dialog.confirm"),
                                120.0,
                                36.0,
                                0.95,
                            )
                            .clicked()
                            {
                                return Some(DialogOutcome::SymlinkProceed {
                                    paths: paths.clone(),
                                    skip_future: skip,
                                });
                            }
                            None
                        });
                    }
                    ActiveDialog::UnlockRecent {
                        path,
                        password,
                        no_password,
                    } => {
                        header(
                            ui,
                            p,
                            Icon::Key,
                            p.accent,
                            crate::ts!("gui.dialog.unlock_recent_title"),
                        );
                        body(ui, p, crate::ts!("gui.dialog.unlock_recent_body"));
                        ui.add_space(8.0);
                        muted(ui, p, path.as_str());
                        ui.add_space(14.0);
                        let response = ui
                            .add_enabled_ui(!*no_password, |ui| {
                                input_field(
                                    ui,
                                    p,
                                    password,
                                    crate::ts!("gui.locked.password_hint"),
                                    Some(Icon::Key),
                                    true,
                                    1.0,
                                )
                            })
                            .inner;
                        // `lost_focus` reads live focus state, so it must be
                        // sampled before the field is re-focused below.
                        let enter =
                            response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        // Focus the password field when the dialog opens, and
                        // again when it is re-shown after a wrong password.
                        let nothing_focused = ui.memory(|mem| mem.focused().is_none());
                        if !enter && (open_t < 0.5 || nothing_focused) && !*no_password {
                            ui.memory_mut(|mem| mem.request_focus(response.id));
                        }
                        ui.add_space(10.0);
                        check_box(ui, p, no_password, crate::ts!("gui.locked.no_password"));
                        ui.add_space(16.0);
                        let submit = actions_row(ui, p, |ui| {
                            if primary_button_sized(
                                ui,
                                p,
                                Some(Icon::Lock),
                                crate::ts!("gui.locked.open_vault"),
                                130.0,
                                36.0,
                                0.95,
                            )
                            .clicked()
                            {
                                return Some(());
                            }
                            None
                        });
                        if submit == Some(Some(())) || enter {
                            outcome = Some(DialogOutcome::UnlockRecent {
                                password: std::mem::take(password),
                                no_password: *no_password,
                            });
                        } else if submit == Some(None) {
                            outcome = Some(DialogOutcome::Cancelled);
                        }
                    }
                    ActiveDialog::MoveOut { paths } => {
                        header(
                            ui,
                            p,
                            Icon::Upload,
                            p.warning,
                            crate::ts!("gui.dialog.move_out_title"),
                        );
                        body(ui, p, crate::ts!("gui.dialog.move_out_body"));
                        ui.add_space(10.0);
                        path_list(ui, p, paths);
                        ui.add_space(18.0);
                        outcome = confirm_row(ui, p, |ui| {
                            if solid_button(
                                ui,
                                crate::ts!("gui.browser.move_out"),
                                p.warning,
                                Color32::WHITE,
                                120.0,
                            )
                            .clicked()
                            {
                                return Some(DialogOutcome::MoveOut {
                                    paths: paths.clone(),
                                });
                            }
                            None
                        });
                    }
                    ActiveDialog::Transfer {
                        copy,
                        paths,
                        destination,
                    } => {
                        let title = if *copy {
                            crate::ts!("gui.dialog.copy_title")
                        } else {
                            crate::ts!("gui.dialog.move_title")
                        };
                        header(
                            ui,
                            p,
                            if *copy { Icon::Copy } else { Icon::Folder },
                            p.accent,
                            title,
                        );
                        body(ui, p, crate::ts!("gui.dialog.transfer_body"));
                        ui.add_space(10.0);
                        input_field(
                            ui,
                            p,
                            destination,
                            crate::ts!("gui.dialog.destination_hint"),
                            Some(Icon::Folder),
                            false,
                            1.0,
                        );
                        ui.add_space(8.0);
                        muted(
                            ui,
                            p,
                            &crate::ts!("gui.browser.selection_count", count = paths.len()),
                        );
                        ui.add_space(16.0);
                        let copy = *copy;
                        let paths = paths.clone();
                        let destination = destination.clone();
                        outcome = confirm_row(ui, p, |ui| {
                            if primary_button_sized(
                                ui,
                                p,
                                None,
                                crate::ts!("gui.dialog.confirm"),
                                120.0,
                                36.0,
                                0.95,
                            )
                            .clicked()
                            {
                                return Some(DialogOutcome::Transfer {
                                    copy,
                                    paths,
                                    destination,
                                });
                            }
                            None
                        });
                    }
                    ActiveDialog::ChangePassword { password, confirm } => {
                        header(
                            ui,
                            p,
                            Icon::Key,
                            p.accent,
                            crate::ts!("gui.dialog.password_title"),
                        );
                        body(ui, p, crate::ts!("gui.dialog.password_body"));
                        ui.add_space(10.0);
                        input_field(
                            ui,
                            p,
                            password,
                            crate::ts!("gui.dialog.password_new"),
                            Some(Icon::Key),
                            true,
                            1.0,
                        );
                        ui.add_space(8.0);
                        input_field(
                            ui,
                            p,
                            confirm,
                            crate::ts!("gui.dialog.password_confirm"),
                            Some(Icon::Key),
                            true,
                            1.0,
                        );
                        ui.add_space(16.0);
                        let password = password.clone();
                        let confirm = confirm.clone();
                        outcome = confirm_row(ui, p, |ui| {
                            if primary_button_sized(
                                ui,
                                p,
                                None,
                                crate::ts!("gui.dialog.confirm"),
                                120.0,
                                36.0,
                                0.95,
                            )
                            .clicked()
                            {
                                return Some(DialogOutcome::ChangePassword { password, confirm });
                            }
                            None
                        });
                    }
                    ActiveDialog::Grow { size } => {
                        header(
                            ui,
                            p,
                            Icon::HardDrive,
                            p.accent,
                            crate::ts!("gui.dialog.grow_title"),
                        );
                        body(ui, p, crate::ts!("gui.dialog.grow_body"));
                        ui.add_space(10.0);
                        input_field(
                            ui,
                            p,
                            size,
                            crate::ts!("gui.dialog.grow_hint"),
                            None,
                            false,
                            1.0,
                        );
                        ui.add_space(16.0);
                        let size = size.clone();
                        outcome = confirm_row(ui, p, |ui| {
                            if primary_button_sized(
                                ui,
                                p,
                                None,
                                crate::ts!("gui.dialog.confirm"),
                                120.0,
                                36.0,
                                0.95,
                            )
                            .clicked()
                            {
                                return Some(DialogOutcome::Grow { size });
                            }
                            None
                        });
                    }
                    ActiveDialog::WritebackConflict { container_path } => {
                        header(
                            ui,
                            p,
                            Icon::Edit,
                            p.warning,
                            crate::ts!("gui.dialog.writeback_title"),
                        );
                        body(
                            ui,
                            p,
                            crate::ts!("gui.dialog.writeback_body", path = container_path.clone()),
                        );
                        ui.add_space(18.0);
                        let resolve =
                            |resolution: ConflictResolution| DialogOutcome::WritebackResolve {
                                container_path: container_path.clone(),
                                resolution,
                            };
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            if primary_button_sized(
                                ui,
                                p,
                                None,
                                crate::ts!("gui.dialog.writeback_overwrite"),
                                120.0,
                                36.0,
                                0.95,
                            )
                            .clicked()
                            {
                                outcome = Some(resolve(ConflictResolution::Overwrite));
                            }
                            if ghost_button(ui, p, crate::ts!("gui.dialog.writeback_keep_both"))
                                .clicked()
                            {
                                outcome = Some(resolve(ConflictResolution::KeepBoth));
                            }
                            if solid_button(
                                ui,
                                crate::ts!("gui.dialog.writeback_discard"),
                                p.danger_soft,
                                p.danger,
                                0.0,
                            )
                            .clicked()
                            {
                                outcome = Some(resolve(ConflictResolution::Discard));
                            }
                        });
                    }
                }
            });
        });

    if outcome.is_none() && escape {
        outcome = Some(DialogOutcome::Cancelled);
    }
    if outcome.is_some() {
        app.dialog = None;
    }
    outcome
}

/// Icon disc + title.
fn header(ui: &mut Ui, p: Palette, icon: Icon, tone: egui::Color32, text: String) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::hover());
        ui.painter().rect_filled(rect, 11.0, with_alpha(tone, 0.16));
        paint_icon(
            ui,
            icon,
            Rect::from_center_size(rect.center(), Vec2::splat(17.0)),
            tone,
        );
        ui.add_space(4.0);
        ui.label(RichText::new(text).size(16.0).strong().color(p.text));
    });
    ui.add_space(12.0);
}

fn body(ui: &mut Ui, p: Palette, text: String) {
    ui.add(egui::Label::new(RichText::new(text).size(13.0).color(p.text)).wrap());
}

fn muted(ui: &mut Ui, p: Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).size(11.5).color(p.muted)).wrap());
}

fn path_list(ui: &mut Ui, p: Palette, paths: &[String]) {
    egui::Frame::new()
        .fill(p.surface_elevated)
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 3.0;
            for path in paths.iter().take(5) {
                ui.add(
                    egui::Label::new(RichText::new(path).size(11.5).monospace().color(p.text))
                        .truncate(),
                );
            }
            if paths.len() > 5 {
                ui.label(
                    RichText::new(crate::ts!(
                        "gui.dialog.delete_batch_more",
                        count = paths.len() - 5
                    ))
                    .size(11.0)
                    .color(p.muted),
                );
            }
        });
}

/// Right-aligned action row: [Cancel] [primary]. The closure renders the
/// primary action and returns `Some(outcome)` when it fires.
///
/// Returns `None` when nothing happened, `Some(Some(x))` when the primary
/// fired, `Some(None)` on cancel.
fn confirm_row(
    ui: &mut Ui,
    p: Palette,
    primary: impl FnOnce(&mut Ui) -> Option<DialogOutcome>,
) -> Option<DialogOutcome> {
    match actions_row(ui, p, primary) {
        Some(Some(outcome)) => Some(outcome),
        Some(None) => Some(DialogOutcome::Cancelled),
        None => None,
    }
}

fn actions_row<T>(
    ui: &mut Ui,
    p: Palette,
    primary: impl FnOnce(&mut Ui) -> Option<T>,
) -> Option<Option<T>> {
    let mut result = None;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if let Some(value) = primary(ui) {
            result = Some(Some(value));
        }
        if ghost_button(ui, p, crate::ts!("gui.dialog.cancel")).clicked() {
            result = Some(None);
        }
    });
    result
}
