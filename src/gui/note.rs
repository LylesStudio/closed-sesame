//! In-vault sticky note. Text stays in memory until an explicit save, then
//! one container write commits it. The card uses the same palette as the vault.

use super::icons::{Icon, paint_icon};
use super::theme::{Palette, with_alpha};
use super::widgets::{ghost_button, primary_button_sized};
use egui::{
    Align, CornerRadius, FontId, Frame, Id, Layout, Margin, Rect, RichText, Sense, Stroke,
    TextEdit, Ui, UiBuilder, pos2, vec2,
};
use zeroize::Zeroize;

/// Notes larger than this open in the external app instead of the card.
pub(crate) const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoteCommand {
    SaveClose,
    Close,
    Discard,
}

pub(crate) struct NoteDraft {
    pub(crate) path: String,
    pub(crate) text: String,
    /// Last text written to the vault. Equal to `text` when nothing is pending.
    pub(crate) saved: String,
    pub(crate) exists: bool,
    focus: bool,
    pending: Option<NoteCommand>,
    pub(crate) banner: String,
}

impl NoteDraft {
    pub(crate) fn fresh(path: String) -> Self {
        Self {
            path,
            text: String::new(),
            saved: String::new(),
            exists: false,
            focus: true,
            pending: None,
            banner: String::new(),
        }
    }

    pub(crate) fn loaded(path: String, text: String) -> Self {
        Self {
            path,
            saved: text.clone(),
            text,
            exists: true,
            focus: true,
            pending: None,
            banner: String::new(),
        }
    }

    pub(crate) fn dirty(&self) -> bool {
        self.text != self.saved
    }

    pub(crate) fn focus_editor(&mut self) {
        self.focus = true;
    }

    pub(crate) fn wipe(&mut self) {
        self.text.zeroize();
        self.saved.zeroize();
        self.path.zeroize();
        self.text.clear();
        self.saved.clear();
        self.path.clear();
        self.banner.clear();
        self.exists = false;
        self.pending = None;
    }

    pub(crate) fn take_pending(&mut self) -> Option<NoteCommand> {
        self.pending.take()
    }
}

impl Drop for NoteDraft {
    fn drop(&mut self) {
        self.wipe();
    }
}

/// Container directory for shortcut-created notes. Empty means the vault root.
pub(crate) fn normalize_note_dir(raw: &str) -> Result<String, ()> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Ok(String::new())
    } else {
        crate::container::path::normalize(trimmed).map_err(|_| ())
    }
}

pub(crate) fn is_text_path(path: &str) -> bool {
    match path.rsplit_once('.') {
        Some((stem, ext)) => {
            !stem.is_empty() && !stem.ends_with('/') && ext.eq_ignore_ascii_case("txt")
        }
        None => false,
    }
}

/// Local editor shortcuts. Only the note window should call this, so the main
/// window keeps Enter, Escape, and its own typing.
pub(crate) fn take_shortcut(ctx: &egui::Context) -> Option<NoteCommand> {
    ctx.input_mut(|input| {
        if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter) {
            Some(NoteCommand::SaveClose)
        } else if input.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
            Some(NoteCommand::Close)
        } else {
            None
        }
    })
}

pub(crate) const WIDTH: f32 = 420.0;
pub(crate) const HEIGHT: f32 = 520.0;

pub(crate) fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("closed-sesame.note")
}

pub(crate) const WINDOW_TITLE: &str = "ClosedSesame.note";

/// Center the note on the monitor under the pointer. Coordinates are points.
pub(crate) fn place_near_cursor(pixels_per_point: f32) -> egui::Pos2 {
    let ppp = pixels_per_point.max(0.1);
    let Some((x, y)) = cursor_physical() else {
        return pos2(96.0, 72.0);
    };
    let Some(area) = super::monitors::work_area_at(x, y) else {
        return pos2(96.0, 72.0);
    };
    let left = area.x / ppp;
    let top = area.y / ppp;
    let width = area.width / ppp;
    let height = area.height / ppp;
    pos2(
        left + (width - WIDTH).max(0.0) * 0.5,
        top + (height - HEIGHT).max(0.0) * 0.5,
    )
}

fn cursor_physical() -> Option<(f64, f64)> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::POINT;
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
        unsafe {
            let mut point = POINT::default();
            GetCursorPos(&mut point).ok()?;
            Some((point.x as f64, point.y as f64))
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Paint the note as its own window and return a command from this frame.
pub(crate) fn show(ui: &mut Ui, draft: &mut NoteDraft, p: Palette) -> Option<NoteCommand> {
    let shortcut = take_shortcut(ui.ctx());
    p.apply(ui.ctx());
    let screen = ui.max_rect();
    ui.painter().rect_filled(screen, 0.0, p.bg);
    card(ui, draft, p, screen.size());
    let pending = draft.take_pending();
    let close_requested = ui.input(|input| input.viewport().close_requested());
    if close_requested {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::CancelClose);
    }
    pending
        .or(shortcut)
        .or(close_requested.then_some(NoteCommand::Close))
}

fn card(ui: &mut Ui, draft: &mut NoteDraft, p: Palette, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::click());
    let painter = ui.painter();
    painter.rect_filled(
        rect.translate(vec2(0.0, 8.0)),
        16.0,
        with_alpha(p.shadow, 0.55),
    );
    painter.rect_filled(rect, 16.0, p.surface);
    painter.rect_stroke(
        rect,
        16.0,
        Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );
    let accent = Rect::from_min_size(rect.min, vec2(rect.width(), 3.0));
    painter.rect_filled(
        accent,
        CornerRadius {
            nw: 16,
            ne: 16,
            sw: 0,
            se: 0,
        },
        p.accent,
    );

    let inner = rect.shrink2(vec2(18.0, 16.0)).translate(vec2(0.0, 4.0));
    let mut content = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    header(&mut content, draft, p);
    content.add_space(8.0);
    let footer_h = if draft.banner.is_empty() { 72.0 } else { 98.0 };
    let body_h = (content.available_height() - footer_h).max(120.0);
    body(&mut content, draft, p, body_h);
    content.add_space(8.0);
    footer(&mut content, draft, p);
}

fn header(ui: &mut Ui, draft: &mut NoteDraft, p: Palette) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let full = ui.available_width();
        let (badge, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
        ui.painter().rect_filled(badge, 8.0, p.accent_soft);
        paint_icon(ui, Icon::Edit, badge.shrink(7.0), p.accent);
        let name_w = (full - 28.0 - 8.0 - 34.0 - if draft.dirty() { 16.0 } else { 0.0 }).max(48.0);
        let title = ui
            .vertical(|ui| {
                ui.set_max_width(name_w);
                ui.label(
                    RichText::new(crate::ts!("gui.note.kicker"))
                        .size(10.5)
                        .color(p.accent)
                        .strong(),
                );
                let file = draft.path.rsplit('/').next().unwrap_or(draft.path.as_str());
                ui.add(
                    egui::Label::new(RichText::new(file).size(15.0).color(p.text))
                        .truncate()
                        .selectable(false),
                );
            })
            .response;
        if title.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if draft.dirty() {
            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 3.5, p.accent);
        }
        let rest = (ui.available_width() - 26.0).max(0.0);
        ui.add_space(rest);
        if super::widgets::flat_icon_button(ui, p, Icon::X, 26.0).clicked() {
            draft.pending = Some(NoteCommand::Close);
        }
    });
    let place = match draft.path.rsplit_once('/') {
        Some((dir, _)) => dir.to_owned(),
        None => crate::ts!("gui.note.root"),
    };
    ui.label(RichText::new(place).size(11.5).color(p.muted));
}

fn body(ui: &mut Ui, draft: &mut NoteDraft, p: Palette, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    ui.painter().rect_filled(rect, 12.0, p.surface_elevated);
    ui.painter().rect_stroke(
        rect,
        12.0,
        Stroke::new(1.0, p.border),
        egui::StrokeKind::Inside,
    );
    let text_rect = rect.shrink2(vec2(12.0, 10.0));
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(text_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    let id = Id::new("sesame.note.body").with(draft.path.as_str());
    if draft.focus {
        child.memory_mut(|mem| mem.request_focus(id));
        draft.focus = false;
    }
    let editor = TextEdit::multiline(&mut draft.text)
        .id(id)
        .frame(Frame::NONE)
        .desired_width(text_rect.width())
        .font(FontId::proportional(15.0))
        .text_color(p.text)
        .hint_text(
            RichText::new(crate::ts!("gui.note.placeholder"))
                .color(p.faint)
                .size(15.0),
        )
        .margin(Margin::symmetric(2, 2));
    let response = child.add_sized(text_rect.size(), editor);
    if response.changed() {
        draft.banner.clear();
    }
}

fn footer(ui: &mut Ui, draft: &mut NoteDraft, p: Palette) {
    if !draft.banner.is_empty() {
        let color = if draft.banner.starts_with('✗') {
            p.danger
        } else if draft.banner.starts_with('✓') {
            p.success
        } else {
            p.warning
        };
        ui.add(egui::Label::new(RichText::new(&draft.banner).size(11.5).color(color)).truncate());
        ui.add_space(4.0);
    }
    ui.label(
        RichText::new(crate::ts!("gui.note.shortcuts"))
            .size(11.0)
            .color(p.faint),
    );
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if draft.dirty() && ghost_button(ui, p, crate::ts!("gui.note.discard")).clicked() {
            draft.pending = Some(NoteCommand::Discard);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if primary_button_sized(
                ui,
                p,
                None,
                crate::ts!("gui.note.save_close"),
                168.0,
                34.0,
                1.0,
            )
            .clicked()
            {
                draft.pending = Some(NoteCommand::SaveClose);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_dir_accepts_empty_and_relative_paths() {
        assert_eq!(normalize_note_dir("  ").unwrap(), "");
        assert_eq!(normalize_note_dir("notes/today").unwrap(), "notes/today");
        assert!(normalize_note_dir("..").is_err());
        assert!(normalize_note_dir("/abs").is_err());
    }

    #[test]
    fn text_paths_are_txt_only() {
        assert!(is_text_path("新建文本.txt"));
        assert!(is_text_path("notes/A.TXT"));
        assert!(!is_text_path("notes"));
        assert!(!is_text_path(".txt"));
        assert!(!is_text_path("notes/file.txt.bak"));
    }

    #[test]
    fn fresh_note_is_clean_until_edited() {
        let mut note = NoteDraft::fresh("新建文本.txt".into());
        assert!(!note.dirty());
        note.text.push_str("hi");
        assert!(note.dirty());
        note.saved.clone_from(&note.text);
        assert!(!note.dirty());
    }
}
