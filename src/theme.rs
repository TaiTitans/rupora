//! Editor themes (roughly modelled after Typora's built-in themes).

use egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeKind {
    #[default]
    Github,
    Night,
    Newsprint,
    Solarized,
}

impl ThemeKind {
    pub const ALL: [ThemeKind; 4] = [ThemeKind::Github, ThemeKind::Night, ThemeKind::Newsprint, ThemeKind::Solarized];

    pub fn name(self) -> &'static str {
        match self {
            ThemeKind::Github => "Github",
            ThemeKind::Night => "Night",
            ThemeKind::Newsprint => "Newsprint",
            ThemeKind::Solarized => "Solarized Dark",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub dark: bool,
    pub bg: Color32,
    pub sidebar_bg: Color32,
    pub text: Color32,
    pub heading: Color32,
    pub muted: Color32,
    pub link: Color32,
    pub accent: Color32,
    pub code_bg: Color32,
    pub code_text: Color32,
    pub quote_bar: Color32,
    pub quote_text: Color32,
    pub rule: Color32,
    pub table_border: Color32,
    pub table_stripe: Color32,
    pub selection: Color32,
    pub active_bg: Color32,
    // syntax highlighting
    pub syn_keyword: Color32,
    pub syn_string: Color32,
    pub syn_comment: Color32,
    pub syn_number: Color32,
    pub syn_type: Color32,
    /// Rendered (non-editing) text uses a serif-ish font in Newsprint.
    pub serif: bool,
}

fn hex(s: u32) -> Color32 {
    Color32::from_rgb((s >> 16) as u8, (s >> 8) as u8, s as u8)
}

impl Theme {
    pub fn get(kind: ThemeKind) -> Theme {
        match kind {
            ThemeKind::Github => Theme {
                dark: false,
                bg: hex(0xffffff),
                sidebar_bg: hex(0xf6f8fa),
                text: hex(0x333333),
                heading: hex(0x111111),
                muted: hex(0x9a9a9a),
                link: hex(0x4183c4),
                accent: hex(0x4183c4),
                code_bg: hex(0xf6f8fa),
                code_text: hex(0x333333),
                quote_bar: hex(0xdfe2e5),
                quote_text: hex(0x777777),
                rule: hex(0xe7e7e7),
                table_border: hex(0xdfe2e5),
                table_stripe: hex(0xf8f8f8),
                selection: Color32::from_rgba_unmultiplied(65, 131, 196, 70),
                active_bg: hex(0xfafbfc),
                syn_keyword: hex(0xa626a4),
                syn_string: hex(0x50a14f),
                syn_comment: hex(0xa0a1a7),
                syn_number: hex(0x986801),
                syn_type: hex(0x4078f2),
                serif: false,
            },
            ThemeKind::Night => Theme {
                dark: true,
                bg: hex(0x363b40),
                sidebar_bg: hex(0x2e3033),
                text: hex(0xb8bfc6),
                heading: hex(0xdedede),
                muted: hex(0x7a8087),
                link: hex(0xe0e0e0),
                accent: hex(0x8cb4ff),
                code_bg: hex(0x333333),
                code_text: hex(0xd0d0d0),
                quote_bar: hex(0x5a6066),
                quote_text: hex(0x9da5ad),
                rule: hex(0x555a60),
                table_border: hex(0x5a6066),
                table_stripe: hex(0x3c4146),
                selection: Color32::from_rgba_unmultiplied(140, 180, 255, 60),
                active_bg: hex(0x3a3f45),
                syn_keyword: hex(0xc792ea),
                syn_string: hex(0xc3e88d),
                syn_comment: hex(0x697098),
                syn_number: hex(0xf78c6c),
                syn_type: hex(0x82aaff),
                serif: false,
            },
            ThemeKind::Newsprint => Theme {
                dark: false,
                bg: hex(0xf3f2ee),
                sidebar_bg: hex(0xeae8e1),
                text: hex(0x1f0909),
                heading: hex(0x1f0909),
                muted: hex(0x8a8070),
                link: hex(0x065588),
                accent: hex(0x065588),
                code_bg: hex(0xdadada),
                code_text: hex(0x1f0909),
                quote_bar: hex(0xb8b1a0),
                quote_text: hex(0x55493c),
                rule: hex(0xc5c0b0),
                table_border: hex(0xc5c0b0),
                table_stripe: hex(0xebe9e2),
                selection: Color32::from_rgba_unmultiplied(6, 85, 136, 60),
                active_bg: hex(0xefeee9),
                syn_keyword: hex(0x7a3e9d),
                syn_string: hex(0x448c27),
                syn_comment: hex(0x8a8070),
                syn_number: hex(0x9c5d27),
                syn_type: hex(0x325cc0),
                serif: true,
            },
            ThemeKind::Solarized => Theme {
                dark: true,
                bg: hex(0x002b36),
                sidebar_bg: hex(0x00222b),
                text: hex(0x93a1a1),
                heading: hex(0xeee8d5),
                muted: hex(0x586e75),
                link: hex(0x268bd2),
                accent: hex(0xb58900),
                code_bg: hex(0x073642),
                code_text: hex(0x93a1a1),
                quote_bar: hex(0x586e75),
                quote_text: hex(0x839496),
                rule: hex(0x2a4a55),
                table_border: hex(0x2a4a55),
                table_stripe: hex(0x04313c),
                selection: Color32::from_rgba_unmultiplied(38, 139, 210, 70),
                active_bg: hex(0x03303b),
                syn_keyword: hex(0x859900),
                syn_string: hex(0x2aa198),
                syn_comment: hex(0x586e75),
                syn_number: hex(0xd33682),
                syn_type: hex(0xb58900),
                serif: false,
            },
        }
    }

    pub fn apply(&self, ctx: &egui::Context) {
        let mut v = if self.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = self.bg;
        v.window_fill = if self.dark { self.sidebar_bg } else { self.bg };
        v.extreme_bg_color = self.bg;
        v.faint_bg_color = self.table_stripe;
        v.code_bg_color = self.code_bg;
        v.hyperlink_color = self.link;
        v.override_text_color = Some(self.text);
        v.selection.bg_fill = self.selection;
        v.selection.stroke.color = self.accent;
        v.text_cursor.stroke.color = self.accent;
        v.widgets.noninteractive.bg_stroke.color = self.rule;
        let theme = if self.dark { egui::Theme::Dark } else { egui::Theme::Light };
        ctx.set_theme(theme);
        ctx.set_visuals_of(theme, v);
        ctx.all_styles_mut(|s| {
            s.interaction.selectable_labels = false;
            s.spacing.item_spacing.y = 6.0;
        });
    }

    /// CSS used for HTML export.
    pub fn export_css(&self) -> String {
        let c = |c: Color32| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
        let font = if self.serif {
            "Georgia, 'Times New Roman', serif"
        } else {
            "-apple-system, 'Segoe UI', 'Helvetica Neue', 'Open Sans', Arial, sans-serif"
        };
        format!(
            r#"body{{background:{bg};color:{text};font-family:{font};font-size:16px;line-height:1.65;max-width:860px;margin:0 auto;padding:30px 30px 100px}}
h1,h2,h3,h4,h5,h6{{color:{heading};line-height:1.3;margin:1.2em 0 .6em}}
h1{{font-size:2.2em;border-bottom:1px solid {rule};padding-bottom:.3em}}
h2{{font-size:1.75em;border-bottom:1px solid {rule};padding-bottom:.3em}}
h3{{font-size:1.4em}} h4{{font-size:1.2em}} h6{{color:{muted}}}
a{{color:{link};text-decoration:none}} a:hover{{text-decoration:underline}}
code{{background:{code_bg};color:{code_text};padding:.15em .35em;border-radius:3px;font-family:'JetBrains Mono',Consolas,Menlo,monospace;font-size:.9em}}
pre{{background:{code_bg};padding:12px 14px;border-radius:4px;overflow:auto}} pre code{{padding:0;background:none}}
blockquote{{border-left:4px solid {quote_bar};color:{quote_text};margin:0;padding:0 15px}}
hr{{border:0;border-top:2px solid {rule};margin:1.5em 0}}
table{{border-collapse:collapse;margin:1em 0}} th,td{{border:1px solid {tb};padding:6px 13px}} tr:nth-child(2n){{background:{stripe}}}
img{{max-width:100%}} li>input[type=checkbox]{{margin-right:.4em}}
"#,
            bg = c(self.bg),
            text = c(self.text),
            heading = c(self.heading),
            rule = c(self.rule),
            muted = c(self.muted),
            link = c(self.link),
            code_bg = c(self.code_bg),
            code_text = c(self.code_text),
            quote_bar = c(self.quote_bar),
            quote_text = c(self.quote_text),
            tb = c(self.table_border),
            stripe = c(self.table_stripe),
        )
    }
}
