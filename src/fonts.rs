//! Font setup: loads system fonts (regular / bold / serif / CJK) when present,
//! falling back to egui's bundled fonts. Every family used by the app is always
//! registered so lookups never panic.

use egui::{FontData, FontDefinitions, FontFamily, FontId};

pub const BOLD: &str = "bold";
pub const SERIF: &str = "serif";

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}
pub fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SERIF.into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

#[derive(Clone, Copy, Debug)]
pub struct Sizes {
    pub body: f32,
    pub mono: f32,
    pub small: f32,
}

impl Sizes {
    pub fn new(body: f32) -> Self {
        Self { body, mono: (body * 0.88).round(), small: (body * 0.8).round() }
    }
}

const REGULAR: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/noto/NotoSans-Regular.ttf",
    "C:\\Windows\\Fonts\\segoeui.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/Library/Fonts/Arial.ttf",
];
const BOLDS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
    "/usr/share/fonts/noto/NotoSans-Bold.ttf",
    "C:\\Windows\\Fonts\\segoeuib.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/Library/Fonts/Arial Bold.ttf",
];
const SERIFS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
    "/usr/share/fonts/TTF/DejaVuSerif.ttf",
    "/usr/share/fonts/dejavu/DejaVuSerif.ttf",
    "/usr/share/fonts/truetype/noto/NotoSerif-Regular.ttf",
    "C:\\Windows\\Fonts\\georgia.ttf",
    "/System/Library/Fonts/Supplemental/Georgia.ttf",
];
const MONOS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "C:\\Windows\\Fonts\\consola.ttf",
    "/System/Library/Fonts/Supplemental/Courier New.ttf",
];
const CJK: &[&str] = &[
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "C:\\Windows\\Fonts\\msyh.ttc",
];

fn load_first(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

/// Returns true if a real bold font was found.
pub fn install(ctx: &egui::Context) -> bool {
    let mut defs = FontDefinitions::default();
    let default_prop = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let default_mono = defs.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();

    let add = |defs: &mut FontDefinitions, name: &str, bytes: Vec<u8>| {
        defs.font_data.insert(name.to_owned(), std::sync::Arc::new(FontData::from_owned(bytes)));
        name.to_owned()
    };

    let regular = load_first(REGULAR).map(|b| add(&mut defs, "sys-regular", b));
    let bold = load_first(BOLDS).map(|b| add(&mut defs, "sys-bold", b));
    let serif = load_first(SERIFS).map(|b| add(&mut defs, "sys-serif", b));
    let mono = load_first(MONOS).map(|b| add(&mut defs, "sys-mono", b));
    let cjk = load_first(CJK).map(|b| add(&mut defs, "sys-cjk", b));

    let with_fallbacks = |first: Option<&String>, base: &[String]| {
        let mut v: Vec<String> = first.into_iter().cloned().collect();
        v.extend(base.iter().cloned());
        if let Some(c) = &cjk {
            v.push(c.clone());
        }
        v
    };

    let prop = with_fallbacks(regular.as_ref(), &default_prop);
    defs.families.insert(FontFamily::Proportional, prop.clone());
    defs.families.insert(FontFamily::Name(BOLD.into()), with_fallbacks(bold.as_ref().or(regular.as_ref()), &default_prop));
    defs.families.insert(FontFamily::Name(SERIF.into()), with_fallbacks(serif.as_ref().or(regular.as_ref()), &default_prop));
    defs.families.insert(FontFamily::Monospace, with_fallbacks(mono.as_ref(), &default_mono));

    ctx.set_fonts(defs);
    bold.is_some()
}
