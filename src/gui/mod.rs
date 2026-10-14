//! Native egui frontend.

mod actions;
mod anti_capture;
mod app;
mod auto_lock;
mod browser;
mod cache;
mod dialog;
mod fonts;
mod icons;
mod job;
mod layout;
mod open_external;
mod recent;
mod selection;
mod settings;
mod theme;
mod tray;
mod views;
mod widgets;

use crate::i18n;
use std::sync::Arc;

/// Compile-time embedded logo for use in the UI.
pub const LOGO: egui::ImageSource<'_> = egui::include_image!("../../assets/logo-mark.png");

fn load_window_icon() -> Option<Arc<egui::IconData>> {
    let png = include_bytes!("../../assets/logo-icon.png");
    let img = image::load_from_memory(png).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(Arc::new(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    }))
}

pub fn run() -> eframe::Result<()> {
    let gui_settings = settings::GuiSettings::load();
    if let Some(locale) = gui_settings.locale() {
        i18n::init(Some(locale));
    }
    let window_title = if gui_settings.locale.is_some() {
        ts!("gui.window_title")
    } else {
        "Closed Sesame".to_owned()
    };

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([layout::DESIGN_WIDTH, layout::DESIGN_HEIGHT])
        .with_min_inner_size([layout::MIN_WINDOW_WIDTH, layout::MIN_WINDOW_HEIGHT]);

    if let Some(icon) = load_window_icon() {
        viewport = viewport.with_icon(icon);
    }

    eframe::run_native(
        &window_title,
        eframe::NativeOptions {
            viewport,
            // The near-black backgrounds use very low-alpha glows; the
            // default gradient dithering turns those into visible speckle.
            dithering: false,
            ..Default::default()
        },
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            fonts::install_fonts(&cc.egui_ctx);
            Ok(Box::new(app::App::new(gui_settings)))
        }),
    )
}
