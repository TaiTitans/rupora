//! Syntax highlighting:
//! * `markdown_job` – styles raw markdown while it is being edited
//!   (headings get bigger, `**markers**` are dimmed …), Typora-style.
//! * `code_job` – lightweight, language-agnostic code highlighting.

use crate::fonts::{bold, mono, regular, Sizes};
use crate::theme::Theme;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId, Stroke};

pub fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 2.0,
        2 => 1.6,
        3 => 1.35,
        4 => 1.15,
        5 => 1.0,
        _ => 0.92,
    }
}

fn fmt(font_id: FontId, color: Color32) -> TextFormat {
    TextFormat { font_id, color, ..Default::default() }
}

#[derive(Clone, Copy, Default)]
struct InlineState {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    math: bool,
}

struct Ctx<'a> {
    theme: &'a Theme,
}

impl Ctx<'_> {
    fn text_fmt(&self, st: InlineState, size: f32, base_color: Color32, heading: bool) -> TextFormat {
        let t = self.theme;
        if st.code || st.math {
            let mut f = fmt(mono(size * 0.92), if st.math { t.accent } else { t.code_text });
            f.background = t.code_bg;
            return f;
        }
        let font = if st.bold || heading { bold(size) } else { regular(size) };
        let color = if st.bold && !heading { t.heading } else { base_color };
        let mut f = fmt(font, color);
        f.italics = st.italic;
        if st.strike {
            f.strikethrough = Stroke::new(1.0, color);
        }
        f
    }

    fn marker_fmt(&self, size: f32) -> TextFormat {
        fmt(regular(size), self.theme.muted)
    }
}

/// Highlight one line of inline markdown, appending to `job`.
fn inline(job: &mut LayoutJob, cx: &Ctx, line: &str, st: &mut InlineState, size: f32, color: Color32, heading: bool) {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let n = chars.len();
    let mut i = 0;
    let mut plain_start = 0usize; // byte index
    let byte_at = |k: usize| if k < n { chars[k].0 } else { line.len() };

    macro_rules! flush {
        ($upto:expr) => {
            if $upto > plain_start {
                job.append(&line[plain_start..$upto], 0.0, cx.text_fmt(*st, size, color, heading));
            }
        };
    }
    macro_rules! marker {
        ($from:expr, $to:expr) => {
            job.append(&line[byte_at($from)..byte_at($to)], 0.0, cx.marker_fmt(size));
        };
    }

    while i < n {
        let c = chars[i].1;
        let next = if i + 1 < n { Some(chars[i + 1].1) } else { None };
        let prev = if i > 0 { Some(chars[i - 1].1) } else { None };
        let b = byte_at(i);

        if st.code {
            if c == '`' {
                flush!(b);
                st.code = false;
                marker!(i, i + 1);
                i += 1;
                plain_start = byte_at(i);
            } else {
                i += 1;
            }
            continue;
        }
        if st.math {
            if c == '$' {
                flush!(b);
                st.math = false;
                marker!(i, i + 1);
                i += 1;
                plain_start = byte_at(i);
            } else {
                i += 1;
            }
            continue;
        }

        match c {
            '\\' if next.is_some_and(|c| c.is_ascii_punctuation()) => {
                flush!(b);
                marker!(i, i + 1);
                plain_start = byte_at(i + 1);
                i += 2;
            }
            '`' => {
                flush!(b);
                marker!(i, i + 1);
                st.code = true;
                i += 1;
                plain_start = byte_at(i);
            }
            '$' if next.is_some_and(|c| !c.is_whitespace() && c != '$') || (st.math && c == '$') => {
                flush!(b);
                marker!(i, i + 1);
                st.math = true;
                i += 1;
                plain_start = byte_at(i);
            }
            '*' | '_' if next == Some(c) => {
                flush!(b);
                marker!(i, i + 2);
                st.bold = !st.bold;
                i += 2;
                plain_start = byte_at(i);
            }
            '*' => {
                flush!(b);
                marker!(i, i + 1);
                st.italic = !st.italic;
                i += 1;
                plain_start = byte_at(i);
            }
            '_' if (!st.italic && !prev.is_some_and(|p| p.is_alphanumeric()))
                || (st.italic && !next.is_some_and(|p| p.is_alphanumeric())) =>
            {
                flush!(b);
                marker!(i, i + 1);
                st.italic = !st.italic;
                i += 1;
                plain_start = byte_at(i);
            }
            '~' if next == Some('~') => {
                flush!(b);
                marker!(i, i + 2);
                st.strike = !st.strike;
                i += 2;
                plain_start = byte_at(i);
            }
            '[' | '!' => {
                // [text](url) or ![alt](url)
                let open = if c == '!' { i + 1 } else { i };
                if c == '!' && next != Some('[') {
                    i += 1;
                    continue;
                }
                let close = (open + 1..n).find(|&k| chars[k].1 == ']');
                let ok = close.and_then(|cl| {
                    if cl + 1 < n && chars[cl + 1].1 == '(' {
                        (cl + 2..n).find(|&k| chars[k].1 == ')').map(|end| (cl, end))
                    } else {
                        None
                    }
                });
                if let Some((cl, end)) = ok {
                    flush!(b);
                    marker!(i, open + 1);
                    let mut lf = cx.text_fmt(*st, size, cx.theme.link, heading);
                    lf.underline = Stroke::new(1.0, cx.theme.link);
                    job.append(&line[byte_at(open + 1)..byte_at(cl)], 0.0, lf);
                    marker!(cl, end + 1);
                    i = end + 1;
                    plain_start = byte_at(i);
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    flush!(line.len());
}

fn is_list_marker(s: &str) -> Option<usize> {
    // returns byte length of "  - " / "12. " (+ "[ ] ")
    let indent = s.len() - s.trim_start().len();
    let t = &s[indent..];
    let mut len = None;
    if (t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ")) || matches!(t, "-" | "*" | "+") {
        len = Some(indent + 2.min(t.len()));
    } else {
        let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && digits < 10 {
            let rest = &t[digits..];
            if rest.starts_with(". ") || rest.starts_with(") ") || rest == "." || rest == ")" {
                len = Some(indent + digits + 2.min(rest.len()));
            }
        }
    }
    let l = len?;
    let after = &s[l..];
    if after.starts_with("[ ] ") || after.starts_with("[x] ") || after.starts_with("[X] ") {
        return Some(l + 4);
    }
    Some(l)
}

fn is_hr(t: &str) -> bool {
    let t = t.trim();
    let compact: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    compact.len() >= 3 && (compact.chars().all(|c| c == '-') || compact.chars().all(|c| c == '*') || compact.chars().all(|c| c == '_'))
}

/// Layout raw markdown with Typora-like source styling.
pub fn markdown_job(text: &str, theme: &Theme, sizes: Sizes, wrap_width: f32, uniform: bool) -> LayoutJob {
    let scale = |l: u8| if uniform { 1.0 } else { heading_scale(l) };
    let cx = Ctx { theme };
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let base = sizes.body;
    let mut fence: Option<String> = None;
    let mut math_block = false;
    let mut st = InlineState::default();
    let lines: Vec<&str> = text.split_inclusive('\n').collect();

    for (li, raw) in lines.iter().enumerate() {
        let line = raw.trim_end_matches('\n');
        let nl = &raw[line.len()..];
        let trimmed = line.trim_start();

        // fenced code
        if let Some(f) = &fence {
            let is_close = trimmed.starts_with(f.as_str()) && trimmed.trim_end().chars().all(|c| c == f.chars().next().unwrap());
            let mut cf = fmt(mono(sizes.mono), if is_close { theme.muted } else { theme.code_text });
            cf.background = theme.code_bg;
            job.append(raw, 0.0, cf);
            if is_close {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let ch = trimmed.chars().next().unwrap();
            let n = trimmed.chars().take_while(|c| *c == ch).count();
            fence = Some(std::iter::repeat_n(ch, n).collect());
            let mut cf = fmt(mono(sizes.mono), theme.muted);
            cf.background = theme.code_bg;
            job.append(raw, 0.0, cf);
            continue;
        }
        // display math
        if trimmed.starts_with("$$") || math_block {
            let toggles = trimmed.matches("$$").count();
            let mut cf = fmt(mono(sizes.mono), theme.accent);
            cf.background = theme.code_bg;
            job.append(raw, 0.0, cf);
            if toggles % 2 == 1 {
                math_block = !math_block;
            }
            continue;
        }
        if line.trim().is_empty() {
            st = InlineState::default();
            job.append(raw, 0.0, fmt(regular(base), theme.text));
            continue;
        }
        // setext underline
        let is_setext = li > 0
            && !lines[li - 1].trim().is_empty()
            && (trimmed.chars().all(|c| c == '=')
                || (trimmed.chars().all(|c| c == '-') && trimmed.len() >= 2 && is_list_marker(line).is_none()));
        if is_setext && !is_hr(lines[li - 1]) {
            job.append(raw, 0.0, cx.marker_fmt(base));
            continue;
        }
        // horizontal rule
        if is_hr(line) && is_list_marker(line).is_none() {
            job.append(raw, 0.0, cx.marker_fmt(base));
            continue;
        }
        // heading (ATX)
        if let Some(level) = crate::document::Document::heading_level(line).filter(|_| trimmed.starts_with('#')) {
            let size = base * scale(level);
            let hashes = line.len() - trimmed.len() + level as usize;
            job.append(&line[..hashes], 0.0, cx.marker_fmt(size));
            let mut hs = InlineState::default();
            inline(&mut job, &cx, &line[hashes..], &mut hs, size, theme.heading, true);
            job.append(nl, 0.0, cx.marker_fmt(size));
            continue;
        }
        // setext heading text (line followed by === / ---)
        if let Some(next) = lines.get(li + 1) {
            let nt = next.trim();
            if !nt.is_empty()
                && (nt.chars().all(|c| c == '=') || (nt.chars().all(|c| c == '-') && nt.len() >= 2))
                && is_list_marker(line).is_none()
            {
                let level = if nt.starts_with('=') { 1 } else { 2 };
                let size = base * scale(level);
                let mut hs = InlineState::default();
                inline(&mut job, &cx, line, &mut hs, size, theme.heading, true);
                job.append(nl, 0.0, cx.marker_fmt(size));
                continue;
            }
        }
        // table row
        if trimmed.starts_with('|') {
            for (k, part) in line.split('|').enumerate() {
                if k > 0 {
                    job.append("|", 0.0, fmt(mono(sizes.mono), theme.muted));
                }
                let is_delim = !part.trim().is_empty() && part.trim().chars().all(|c| matches!(c, '-' | ':'));
                job.append(part, 0.0, fmt(mono(sizes.mono), if is_delim { theme.muted } else { theme.text }));
            }
            job.append(nl, 0.0, fmt(mono(sizes.mono), theme.text));
            continue;
        }
        // blockquote prefix
        let mut rest = line;
        let mut color = theme.text;
        let q_end = {
            let mut k = 0;
            let bytes = line.as_bytes();
            loop {
                while k < bytes.len() && bytes[k] == b' ' {
                    k += 1;
                }
                if k < bytes.len() && bytes[k] == b'>' {
                    k += 1;
                    if k < bytes.len() && bytes[k] == b' ' {
                        k += 1;
                    }
                    color = theme.quote_text;
                } else {
                    break;
                }
            }
            if color == theme.quote_text {
                k
            } else {
                0
            }
        };
        if q_end > 0 {
            job.append(&line[..q_end], 0.0, fmt(bold(base), theme.quote_bar));
            rest = &line[q_end..];
        }
        if let Some(ml) = is_list_marker(rest) {
            job.append(&rest[..ml], 0.0, fmt(bold(base), theme.accent));
            rest = &rest[ml..];
        }
        inline(&mut job, &cx, rest, &mut st, base, color, false);
        job.append(nl, 0.0, fmt(regular(base), theme.text));
    }
    if text.is_empty() {
        job.append("", 0.0, fmt(regular(base), theme.text));
    }
    job
}

// ───────────────────────── code highlighting ─────────────────────────

const KEYWORDS: &[&str] = &[
    "fn",
    "let",
    "mut",
    "pub",
    "struct",
    "enum",
    "impl",
    "trait",
    "use",
    "mod",
    "crate",
    "super",
    "where",
    "dyn",
    "ref",
    "move",
    "unsafe",
    "extern",
    "match",
    "loop",
    "if",
    "else",
    "for",
    "while",
    "do",
    "return",
    "break",
    "continue",
    "in",
    "of",
    "as",
    "true",
    "false",
    "null",
    "nil",
    "None",
    "Some",
    "Ok",
    "Err",
    "self",
    "Self",
    "this",
    "new",
    "delete",
    "def",
    "class",
    "import",
    "from",
    "export",
    "default",
    "const",
    "var",
    "function",
    "async",
    "await",
    "yield",
    "try",
    "catch",
    "finally",
    "throw",
    "throws",
    "raise",
    "except",
    "with",
    "pass",
    "lambda",
    "elif",
    "not",
    "and",
    "or",
    "is",
    "int",
    "float",
    "double",
    "bool",
    "boolean",
    "char",
    "string",
    "str",
    "void",
    "static",
    "public",
    "private",
    "protected",
    "internal",
    "interface",
    "type",
    "typeof",
    "instanceof",
    "package",
    "func",
    "go",
    "chan",
    "select",
    "switch",
    "case",
    "defer",
    "struct",
    "extends",
    "implements",
    "abstract",
    "final",
    "override",
    "virtual",
    "namespace",
    "using",
    "template",
    "typename",
    "auto",
    "unsigned",
    "signed",
    "long",
    "short",
    "u8",
    "u16",
    "u32",
    "u64",
    "usize",
    "i8",
    "i16",
    "i32",
    "i64",
    "isize",
    "f32",
    "f64",
    "String",
    "Vec",
    "Option",
    "Result",
    "Box",
    "echo",
    "then",
    "fi",
    "done",
    "esac",
    "local",
    "export",
    "SELECT",
    "FROM",
    "WHERE",
    "INSERT",
    "INTO",
    "UPDATE",
    "DELETE",
    "CREATE",
    "TABLE",
    "JOIN",
    "ON",
    "GROUP",
    "BY",
    "ORDER",
    "LIMIT",
    "AND",
    "OR",
    "NOT",
    "VALUES",
    "SET",
    "end",
    "begin",
    "require",
    "module",
    "val",
    "fun",
    "object",
    "when",
    "undefined",
    "let!",
    "macro_rules!",
    "println!",
    "vec!",
    "format!",
];

#[derive(Clone, Copy, PartialEq)]
enum Comment {
    Slash,
    Hash,
    Dash,
    None,
}

fn comment_style(lang: &str) -> Comment {
    match lang.to_ascii_lowercase().as_str() {
        "rust" | "rs" | "c" | "h" | "cpp" | "c++" | "cc" | "hpp" | "java" | "js" | "javascript" | "jsx" | "ts" | "typescript" | "tsx"
        | "go" | "golang" | "swift" | "kotlin" | "kt" | "cs" | "csharp" | "c#" | "php" | "scala" | "dart" | "css" | "scss" | "less"
        | "json5" | "jsonc" | "zig" | "groovy" | "glsl" | "wgsl" | "proto" | "solidity" | "sol" => Comment::Slash,
        "python" | "py" | "sh" | "bash" | "shell" | "zsh" | "fish" | "ruby" | "rb" | "yaml" | "yml" | "toml" | "r" | "perl" | "pl"
        | "dockerfile" | "makefile" | "make" | "ini" | "conf" | "cmake" | "nim" | "elixir" | "ex" | "powershell" | "ps1" | "julia"
        | "jl" | "gitignore" | "env" | "nix" => Comment::Hash,
        "sql" | "lua" | "haskell" | "hs" | "elm" | "ada" | "vhdl" | "mysql" | "postgresql" | "psql" => Comment::Dash,
        _ => Comment::None,
    }
}

/// Highlight a code block. `lang` may be empty.
pub fn code_job(lang: &str, code: &str, theme: &Theme, size: f32, wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let font = mono(size);
    let f = |c: Color32| fmt(font.clone(), c);
    let cs = comment_style(lang);
    let lower = lang.to_ascii_lowercase();
    let highlight = !matches!(lower.as_str(), "" | "text" | "txt" | "plain" | "plaintext" | "markdown" | "md");
    if !highlight {
        job.append(code, 0.0, f(theme.code_text));
        return job;
    }
    let rust = matches!(lower.as_str(), "rust" | "rs");
    let chars: Vec<(usize, char)> = code.char_indices().collect();
    let n = chars.len();
    let at = |k: usize| if k < n { chars[k].0 } else { code.len() };
    let ch = |k: usize| if k < n { chars[k].1 } else { '\0' };
    let mut i = 0;
    let mut in_block_comment = false;
    while i < n {
        let c = ch(i);
        let start = i;
        if in_block_comment {
            while i < n && !(ch(i) == '*' && ch(i + 1) == '/') {
                i += 1;
            }
            i = (i + 2).min(n);
            in_block_comment = false;
            job.append(&code[at(start)..at(i)], 0.0, f(theme.syn_comment));
            continue;
        }
        let line_comment = match cs {
            Comment::Slash => c == '/' && ch(i + 1) == '/',
            Comment::Hash => c == '#' && !(ch(i + 1) == '[' || ch(i + 1) == '!') || (c == '#' && i == 0 && ch(i + 1) == '!'),
            Comment::Dash => c == '-' && ch(i + 1) == '-',
            Comment::None => false,
        };
        if line_comment {
            while i < n && ch(i) != '\n' {
                i += 1;
            }
            job.append(&code[at(start)..at(i)], 0.0, f(theme.syn_comment));
            continue;
        }
        if cs == Comment::Slash && c == '/' && ch(i + 1) == '*' {
            in_block_comment = true;
            i += 2;
            while i < n && !(ch(i) == '*' && ch(i + 1) == '/') {
                i += 1;
            }
            i = (i + 2).min(n);
            in_block_comment = false;
            job.append(&code[at(start)..at(i)], 0.0, f(theme.syn_comment));
            continue;
        }
        if c == '"' || c == '`' || (c == '\'' && (!rust || (ch(i + 2) == '\'' || (ch(i + 1) == '\\' && ch(i + 3) == '\'')))) {
            let q = c;
            i += 1;
            while i < n && ch(i) != q {
                if ch(i) == '\\' {
                    i += 1;
                }
                if ch(i) == '\n' && q != '`' && q != '"' {
                    break;
                }
                i += 1;
            }
            i = (i + 1).min(n);
            job.append(&code[at(start)..at(i)], 0.0, f(theme.syn_string));
            continue;
        }
        if c.is_ascii_digit() {
            while i < n && (ch(i).is_ascii_alphanumeric() || ch(i) == '.' || ch(i) == '_') {
                i += 1;
            }
            job.append(&code[at(start)..at(i)], 0.0, f(theme.syn_number));
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == '@' {
            i += 1;
            while i < n && (ch(i).is_alphanumeric() || ch(i) == '_') {
                i += 1;
            }
            if ch(i) == '!' && rust {
                i += 1;
            }
            let word = &code[at(start)..at(i)];
            let color = if KEYWORDS.contains(&word) || word.starts_with('@') || word.ends_with('!') {
                theme.syn_keyword
            } else if word.chars().next().is_some_and(|c| c.is_uppercase()) {
                theme.syn_type
            } else if ch(i) == '(' {
                theme.link
            } else {
                theme.code_text
            };
            job.append(word, 0.0, f(color));
            continue;
        }
        // other run of punctuation / whitespace
        i += 1;
        while i < n && !ch(i).is_alphanumeric() && !matches!(ch(i), '"' | '\'' | '`' | '/' | '#' | '-' | '_' | '@') {
            i += 1;
        }
        job.append(&code[at(start)..at(i)], 0.0, f(theme.code_text));
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Theme, ThemeKind};

    fn job_text(job: &LayoutJob) -> &str {
        &job.text
    }

    #[test]
    fn markdown_job_preserves_text() {
        let theme = Theme::get(ThemeKind::Github);
        let sizes = Sizes::new(16.0);
        for s in [
            "# Hello **world**",
            "para with `code` and *it* and ~~del~~ and [link](http://x) ![img](a.png)\nsecond line\\* esc",
            "```rust\nfn main() {}\n```\nafter",
            "> quote **b**\n> - [ ] task\n\n| a | b |\n|---|---|\n",
            "Setext\n===\n\n---\n\n$$\nx^2\n$$\n$a+b$ snake_case_word _it_",
            "Café **naïve** _résumé_",
            "",
            "unclosed **bold and `code",
        ] {
            let job = markdown_job(s, &theme, sizes, 500.0, false);
            assert_eq!(job_text(&markdown_job(s, &theme, sizes, 500.0, true)), s);
            assert_eq!(job_text(&job), s);
        }
    }

    #[test]
    fn code_job_preserves_text() {
        let theme = Theme::get(ThemeKind::Night);
        for (lang, code) in [
            (
                "rust",
                "fn main() {\n    let s = \"hi\\\"\"; // c\n    let c = 'x'; let l: &'a str;\n    println!(\"{}\", 0x1F);\n}\n/* block */",
            ),
            ("python", "def f(x):\n    # comment\n    return 'a' + \"b\"\n"),
            ("sql", "SELECT * FROM t -- c\n"),
            ("", "plain"),
            ("js", "const t = `tpl ${x}`; /* unclosed"),
            ("bash", "#!/bin/bash\necho \"$HOME\" # hi"),
        ] {
            let job = code_job(lang, code, &theme, 14.0, 500.0);
            assert_eq!(job_text(&job), code);
        }
    }
}
