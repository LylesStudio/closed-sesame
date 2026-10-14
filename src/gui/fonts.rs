//! Embedded CJK font setup for egui.

use egui::{FontData, FontDefinitions, FontFamily};

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "noto_cjk".to_owned(),
        FontData::from_static(include_bytes!("../../assets/fonts/NotoSansSC-Regular.otf")).into(),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "noto_cjk".to_owned());

    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "noto_cjk".to_owned());

    ctx.set_fonts(fonts);
}
