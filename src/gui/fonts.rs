//! Embedded UI fonts.
//!
//! Latin is Source Sans 3. Simplified Chinese is Noto Sans CJK SC and Japanese
//! is Noto Sans CJK JP (思源黑体). The Latin face is first, so it draws English;
//! it has no Han or kana, so those fall through to the CJK face. Japanese UI
//! prefers the JP face; Chinese and English prefer SC.

use crate::i18n::Locale;
use egui::{FontData, FontDefinitions, FontFamily};

const LATIN: &str = "source_sans_3";
const SC: &str = "noto_sans_cjk_sc";
const JP: &str = "noto_sans_cjk_jp";

pub fn install_fonts(ctx: &egui::Context, locale: Locale) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        LATIN.to_owned(),
        FontData::from_static(include_bytes!("../../assets/fonts/SourceSans3-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        SC.to_owned(),
        FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoSansCJKsc-Regular.otf"
        ))
        .into(),
    );
    fonts.font_data.insert(
        JP.to_owned(),
        FontData::from_static(include_bytes!(
            "../../assets/fonts/NotoSansCJKjp-Regular.otf"
        ))
        .into(),
    );

    // Insert in reverse so the preferred face ends up first.
    let proportional: [&str; 3] = match locale {
        Locale::Ja => [SC, JP, LATIN],
        Locale::ZhCn | Locale::En => [JP, SC, LATIN],
    };
    let list = fonts.families.entry(FontFamily::Proportional).or_default();
    for name in proportional {
        list.insert(0, name.to_owned());
    }

    // Keep the built-in monospace for ASCII. CJK falls through to the sans faces.
    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    let mono_cjk: [&str; 2] = match locale {
        Locale::Ja => [JP, SC],
        Locale::ZhCn | Locale::En => [SC, JP],
    };
    for name in mono_cjk {
        mono.push(name.to_owned());
    }

    ctx.set_fonts(fonts);
}
