//! Self-drawn SVG icon set (assets/icons/svg), rasterized to PNG and embedded.
//!
//! Icons are white-on-transparent so they can be tinted to any theme color at
//! render time via [`icon_image`]. Replaces the previous Lucide icon font.

use egui::{Color32, Image, ImageSource, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Icon {
    File,
    Folder,
    FolderOpen,
    FolderLock,
    Settings,
    Lock,
    Plus,
    Refresh,
    Trash,
    Edit,
    ExternalLink,
    Download,
    Upload,
    HardDrive,
    X,
    Key,
    Shield,
    Eye,
    Mount,
    Copy,
}

impl Icon {
    pub(crate) fn source(self) -> ImageSource<'static> {
        match self {
            Self::File => egui::include_image!("../../assets/icons/file.png"),
            Self::Folder => egui::include_image!("../../assets/icons/folder.png"),
            Self::FolderOpen => egui::include_image!("../../assets/icons/folder-open.png"),
            Self::FolderLock => egui::include_image!("../../assets/icons/folder-lock.png"),
            Self::Settings => egui::include_image!("../../assets/icons/settings.png"),
            Self::Lock => egui::include_image!("../../assets/icons/lock.png"),
            Self::Plus => egui::include_image!("../../assets/icons/plus.png"),
            Self::Refresh => egui::include_image!("../../assets/icons/refresh.png"),
            Self::Trash => egui::include_image!("../../assets/icons/trash.png"),
            Self::Edit => egui::include_image!("../../assets/icons/edit.png"),
            Self::ExternalLink => egui::include_image!("../../assets/icons/external-link.png"),
            Self::Download => egui::include_image!("../../assets/icons/download.png"),
            Self::Upload => egui::include_image!("../../assets/icons/upload.png"),
            Self::HardDrive => egui::include_image!("../../assets/icons/hard-drive.png"),
            Self::X => egui::include_image!("../../assets/icons/x.png"),
            Self::Key => egui::include_image!("../../assets/icons/key.png"),
            Self::Shield => egui::include_image!("../../assets/icons/shield.png"),
            Self::Eye => egui::include_image!("../../assets/icons/eye.png"),
            Self::Mount => egui::include_image!("../../assets/icons/mount.png"),
            Self::Copy => egui::include_image!("../../assets/icons/copy.png"),
        }
    }
}

/// A themed, tinted icon image ready for `ui.add(...)` / `Button::image(...)`.
pub(crate) fn icon_image(icon: Icon, size: f32, color: Color32) -> Image<'static> {
    Image::new(icon.source())
        .fit_to_exact_size(Vec2::splat(size))
        .tint(color)
}

/// Paint an icon centered in `rect` (for custom row painting).
pub(crate) fn paint_icon(ui: &mut egui::Ui, icon: Icon, rect: egui::Rect, color: Color32) {
    icon_image(icon, rect.height(), color).paint_at(ui, rect);
}
