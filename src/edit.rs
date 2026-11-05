//! Pure text transformations used by the editor commands
//! (headings, lists, quotes, inline formatting, Enter handling).

use crate::document::{byte_to_char, char_to_byte};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Paragraph,
    Heading,
    List,
    Quote,
    Code,
    Math,
    Table,
    Html,
    Meta,
}

pub fn block_kind(text: &str) -> Kind {
    let first = text.lines().next().unwrap_or("");
    let t = first.trim_start();
    let indent = first.len() - t.len();
    if t.starts_with("```") || t.starts_with("~~~") {
        Kind::Code
    } else if indent >= 4 && !t.is_empty() {
        Kind::Code
    } else if t.starts_with("$$") {
        Kind::Math
    } else if t.starts_with('|') {
        Kind::Table
    } else if t.starts_with('<') {
        Kind::Html
    } else if first == "---" && text.lines().count() > 1 {
        Kind::Meta
    } else if crate::document::Document::heading_level(text).is_some() && t.starts_with('#') {
        Kind::Heading
    } else if list_marker_len(first).is_some() {
        Kind::List
    } else if t.starts_with('>') {
        Kind::Quote
    } else {
        Kind::Paragraph
    }
}

/// Length in bytes of a list marker at the start of `line`, including the
/// indentation, trailing space and an optional task box (`- [ ] `).
pub fn list_marker_len(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let t = &line[indent..];
    let base = if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
        indent + 2
    } else if matches!(t, "-" | "*" | "+") {
        indent + 1
    } else {
        let d = t.chars().take_while(|c| c.is_ascii_digit()).count();
        if d == 0 || d > 9 {
            return None;
        }
        let r = &t[d..];
        if r.starts_with(". ") || r.starts_with(") ") {
            indent + d + 2
        } else if r == "." || r == ")" {
            indent + d + 1
        } else {
            return None;
        }
    };
    let after = &line[base..];
    for tb in ["[ ] ", "[x] ", "[X] "] {
        if after.starts_with(tb) {
            return Some(base + 4);
        }
    }
    for tb in ["[ ]", "[x]", "[X]"] {
        if after == tb {
            return Some(base + 3);
        }
    }
    Some(base)
}

/// The marker to insert on the next line when pressing Enter in a list item.
pub fn next_marker(line: &str) -> Option<String> {
    let ml = list_marker_len(line)?;
    let marker = &line[..ml];
    let indent_len = marker.len() - marker.trim_start_matches([' ', '\t']).len();
    let indent = &marker[..indent_len];
    let m = &marker[indent_len..];
    let task = m.contains("[ ]") || m.contains("[x]") || m.contains("[X]");
    let d = m.chars().take_while(|c| c.is_ascii_digit()).count();
    let mut out = String::from(indent);
    if d > 0 {
        let n: u64 = m[..d].parse().unwrap_or(0);
        let delim = &m[d..d + 1];
        out.push_str(&format!("{}{} ", n + 1, delim));
    } else {
        out.push_str(&m[..1]);
        out.push(' ');
    }
    if task {
        out.push_str("[ ] ");
    }
    Some(out)
}

/// Blockquote prefix (`> `, `> > `) at the start of a line.
pub fn quote_prefix(line: &str) -> &str {
    let b = line.as_bytes();
    let mut k = 0;
    let mut end = 0;
    loop {
        while k < b.len() && b[k] == b' ' {
            k += 1;
        }
        if k < b.len() && b[k] == b'>' {
            k += 1;
            if k < b.len() && b[k] == b' ' {
                k += 1;
            }
            end = k;
        } else {
            break;
        }
    }
    &line[..end]
}

pub fn set_heading(s: &str, level: u8) -> String {
    let mut lines: Vec<&str> = s.split('\n').collect();
    // drop a setext underline
    if lines.len() == 2 {
        let u = lines[1].trim();
        if !u.is_empty() && (u.chars().all(|c| c == '=') || u.chars().all(|c| c == '-')) {
            lines.pop();
        }
    }
    let first = lines.first().copied().unwrap_or("");
    let t = first.trim_start();
    let current = crate::document::Document::heading_level(first).filter(|_| t.starts_with('#'));
    let content = if current.is_some() { t.trim_start_matches('#').trim_start() } else { strip_block_prefix(first) };
    let new_level = if current == Some(level) { 0 } else { level };
    let head = if new_level == 0 { content.to_string() } else { format!("{} {}", "#".repeat(new_level as usize), content) };
    let mut out = head;
    for l in &lines[1..] {
        out.push('\n');
        out.push_str(l);
    }
    out
}

fn strip_block_prefix(line: &str) -> &str {
    let q = quote_prefix(line);
    let rest = &line[q.len()..];
    match list_marker_len(rest) {
        Some(m) => &rest[m..],
        None => rest,
    }
}

pub fn toggle_quote(s: &str) -> String {
    let all_quoted = s.lines().filter(|l| !l.trim().is_empty()).all(|l| l.trim_start().starts_with('>'));
    s.split('\n')
        .map(|l| {
            if all_quoted {
                let t = l.trim_start();
                let t = t.strip_prefix('>').unwrap_or(t);
                t.strip_prefix(' ').unwrap_or(t).to_string()
            } else if l.is_empty() {
                ">".to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    Bullet,
    Ordered,
    Task,
}

fn line_list_kind(l: &str) -> Option<ListKind> {
    let ml = list_marker_len(l)?;
    let m = l[..ml].trim();
    if m.contains('[') {
        Some(ListKind::Task)
    } else if m.starts_with(|c: char| c.is_ascii_digit()) {
        Some(ListKind::Ordered)
    } else {
        Some(ListKind::Bullet)
    }
}

pub fn toggle_list(s: &str, kind: ListKind) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    let non_empty: Vec<&&str> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
    let all_same = !non_empty.is_empty() && non_empty.iter().all(|l| line_list_kind(l) == Some(kind));
    let mut n = 0;
    lines
        .iter()
        .map(|l| {
            if l.trim().is_empty() {
                return l.to_string();
            }
            let indent_len = l.len() - l.trim_start().len();
            let indent = &l[..indent_len];
            let content = match list_marker_len(l) {
                Some(m) => &l[m..],
                None => l.trim_start(),
            };
            if all_same {
                return format!("{indent}{content}");
            }
            n += 1;
            match kind {
                ListKind::Bullet => format!("{indent}- {content}"),
                ListKind::Ordered => format!("{indent}{n}. {content}"),
                ListKind::Task => format!("{indent}- [ ] {content}"),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn toggle_fence(s: &str, fence: &str) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    if lines.len() >= 2 && lines[0].trim_start().starts_with(fence) && lines.last().unwrap().trim() == fence {
        return lines[1..lines.len() - 1].join("\n");
    }
    if s.is_empty() {
        return format!("{fence}\n\n{fence}");
    }
    format!("{fence}\n{s}\n{fence}")
}

/// Wrap/unwrap the char selection `sel` with `marker`. Returns new char selection.
pub fn wrap_inline(text: &mut String, sel: Range<usize>, marker: &str) -> Range<usize> {
    let mc = marker.chars().count();
    let bs = char_to_byte(text, sel.start);
    let be = char_to_byte(text, sel.end);
    // Already wrapped around the selection → unwrap.
    if text[..bs].ends_with(marker) && text[be..].starts_with(marker) && (bs != be || mc > 0) {
        let outer = text[..bs].len() - marker.len();
        text.replace_range(be..be + marker.len(), "");
        text.replace_range(outer..bs, "");
        return sel.start - mc..sel.end - mc;
    }
    // Selection itself includes the markers → unwrap.
    let inner = &text[bs..be];
    if inner.len() >= 2 * marker.len() && inner.starts_with(marker) && inner.ends_with(marker) && bs != be {
        let new_inner = inner[marker.len()..inner.len() - marker.len()].to_string();
        text.replace_range(bs..be, &new_inner);
        return sel.start..sel.end - 2 * mc;
    }
    text.insert_str(be, marker);
    text.insert_str(bs, marker);
    sel.start + mc..sel.end + mc
}

/// Turn the selection into `[sel](|)`, placing the cursor inside the parens.
pub fn make_link(text: &mut String, sel: Range<usize>) -> Range<usize> {
    let bs = char_to_byte(text, sel.start);
    let be = char_to_byte(text, sel.end);
    let inner = text[bs..be].to_string();
    let is_url = inner.starts_with("http://") || inner.starts_with("https://");
    let rep = if is_url { format!("[]({inner})") } else { format!("[{inner}]()") };
    text.replace_range(bs..be, &rep);
    let pos = if is_url { sel.start + 1 } else { sel.start + inner.chars().count() + 3 };
    pos..pos
}

/// Byte range of the "paragraph" (run of non-blank lines) containing byte `b`.
pub fn para_bounds(text: &str, b: usize) -> Range<usize> {
    let b = b.min(text.len());
    let start = text[..b].rfind("\n\n").map(|i| i + 2).unwrap_or(0);
    let end = text[b..].find("\n\n").map(|i| b + i).unwrap_or(text.len());
    start.min(end)..end
}

/// Find all matches of `q` (byte ranges).
pub fn find_all(text: &str, q: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    if q.is_empty() {
        return Vec::new();
    }
    if case_sensitive {
        return text.match_indices(q).map(|(i, m)| i..i + m.len()).collect();
    }
    // Case-insensitive search that keeps byte offsets of the original text.
    let ql: Vec<char> = q.chars().flat_map(|c| c.to_lowercase()).collect();
    let tc: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < tc.len() {
        let mut k = 0;
        let mut j = i;
        let mut ok = true;
        while k < ql.len() {
            if j >= tc.len() {
                ok = false;
                break;
            }
            let lc: Vec<char> = tc[j].1.to_lowercase().collect();
            if ql[k..].starts_with(&lc) {
                k += lc.len();
                j += 1;
            } else {
                ok = false;
                break;
            }
        }
        if ok {
            let end = if j < tc.len() { tc[j].0 } else { text.len() };
            out.push(tc[i].0..end);
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

/// Result of pressing Enter inside a block.
#[derive(Debug, PartialEq, Eq)]
pub enum EnterAction {
    /// Keep the newline as typed.
    Keep,
    /// Replace the block's text and put the cursor at char index.
    Rewrite { text: String, cursor: usize },
    /// Split: block keeps `before`, new block gets `after`.
    Split { before: String, after: String },
}

/// `text` already contains the newline the user typed; `c` is the char index
/// right after that newline.
pub fn on_enter(text: &str, c: usize, shift: bool) -> EnterAction {
    let b = char_to_byte(text, c);
    if b == 0 || text.as_bytes().get(b - 1) != Some(&b'\n') {
        return EnterAction::Keep;
    }
    let before = &text[..b - 1];
    let after = &text[b..];
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line = &before[line_start..];
    match block_kind(text) {
        Kind::Code | Kind::Math | Kind::Table | Kind::Html | Kind::Meta => EnterAction::Keep,
        Kind::Paragraph | Kind::Heading => {
            if shift {
                EnterAction::Keep
            } else {
                EnterAction::Split { before: before.to_string(), after: after.to_string() }
            }
        }
        Kind::List | Kind::Quote => {
            let q = quote_prefix(line);
            let rest = &line[q.len()..];
            if let Some(ml) = list_marker_len(rest) {
                if rest[ml..].trim().is_empty() {
                    // empty item → leave the list
                    let head = before[..line_start].trim_end_matches('\n');
                    return EnterAction::Split { before: head.to_string(), after: after.to_string() };
                }
                if shift {
                    // continuation line inside the item, aligned to content
                    let pad = format!("{q}{}", " ".repeat(ml));
                    let mut t = text.to_string();
                    t.insert_str(b, &pad);
                    return EnterAction::Rewrite { text: t, cursor: c + pad.chars().count() };
                }
                let marker = format!("{q}{}", next_marker(rest).unwrap_or_default());
                let mut t = text.to_string();
                t.insert_str(b, &marker);
                return EnterAction::Rewrite { text: t, cursor: c + marker.chars().count() };
            }
            if !q.is_empty() {
                if rest.trim().is_empty() {
                    let head = before[..line_start].trim_end_matches('\n');
                    return EnterAction::Split { before: head.to_string(), after: after.to_string() };
                }
                let mut t = text.to_string();
                t.insert_str(b, q);
                return EnterAction::Rewrite { text: t, cursor: c + q.chars().count() };
            }
            EnterAction::Keep
        }
    }
}

pub fn char_len(s: &str) -> usize {
    s.chars().count()
}

#[allow(dead_code)]
pub fn byte_range_to_chars(s: &str, r: Range<usize>) -> Range<usize> {
    byte_to_char(s, r.start)..byte_to_char(s, r.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(block_kind("# h"), Kind::Heading);
        assert_eq!(block_kind("- a"), Kind::List);
        assert_eq!(block_kind("1. a"), Kind::List);
        assert_eq!(block_kind("> a"), Kind::Quote);
        assert_eq!(block_kind("```\nx\n```"), Kind::Code);
        assert_eq!(block_kind("| a |"), Kind::Table);
        assert_eq!(block_kind("hello"), Kind::Paragraph);
        assert_eq!(block_kind("---\ntitle: x\n---"), Kind::Meta);
    }

    #[test]
    fn markers() {
        assert_eq!(list_marker_len("- a"), Some(2));
        assert_eq!(list_marker_len("  12. a"), Some(6));
        assert_eq!(list_marker_len("- [ ] a"), Some(6));
        assert_eq!(list_marker_len("-a"), None);
        assert_eq!(next_marker("- [x] done").as_deref(), Some("- [ ] "));
        assert_eq!(next_marker("  9) x").as_deref(), Some("  10) "));
        assert_eq!(next_marker("* x").as_deref(), Some("* "));
    }

    #[test]
    fn headings() {
        assert_eq!(set_heading("hello", 2), "## hello");
        assert_eq!(set_heading("## hello", 2), "hello");
        assert_eq!(set_heading("### hello", 1), "# hello");
        assert_eq!(set_heading("Title\n===", 3), "### Title");
        assert_eq!(set_heading("- item", 1), "# item");
        assert_eq!(set_heading("# x", 0), "x");
    }

    #[test]
    fn quotes_and_lists() {
        assert_eq!(toggle_quote("a\n\nb"), "> a\n>\n> b");
        assert_eq!(toggle_quote("> a\n> b"), "a\nb");
        assert_eq!(toggle_list("a\nb", ListKind::Ordered), "1. a\n2. b");
        assert_eq!(toggle_list("1. a\n2. b", ListKind::Ordered), "a\nb");
        assert_eq!(toggle_list("- a\n- b", ListKind::Task), "- [ ] a\n- [ ] b");
        assert_eq!(toggle_fence("x", "```"), "```\nx\n```");
        assert_eq!(toggle_fence("```\nx\n```", "```"), "x");
    }

    #[test]
    fn inline_wrap() {
        let mut s = "say hello now".to_string();
        let r = wrap_inline(&mut s, 4..9, "**");
        assert_eq!(s, "say **hello** now");
        assert_eq!(r, 6..11);
        let r2 = wrap_inline(&mut s, r, "**");
        assert_eq!(s, "say hello now");
        assert_eq!(r2, 4..9);
        let mut e = "ab".to_string();
        let r = wrap_inline(&mut e, 1..1, "`");
        assert_eq!(e, "a``b");
        assert_eq!(r, 2..2);
        let mut v = "naïve café".to_string();
        wrap_inline(&mut v, 6..10, "*");
        assert_eq!(v, "naïve *café*");
        let mut l = "go here".to_string();
        let r = make_link(&mut l, 3..7);
        assert_eq!(l, "go [here]()");
        assert_eq!(r, 10..10);
    }

    #[test]
    fn enter_behaviour() {
        assert_eq!(on_enter("ab\ncd", 3, false), EnterAction::Split { before: "ab".into(), after: "cd".into() });
        assert_eq!(on_enter("ab\ncd", 3, true), EnterAction::Keep);
        assert_eq!(on_enter("- a\n", 4, false), EnterAction::Rewrite { text: "- a\n- ".into(), cursor: 6 });
        assert_eq!(on_enter("1. a\n", 5, false), EnterAction::Rewrite { text: "1. a\n2. ".into(), cursor: 8 });
        assert_eq!(on_enter("- a\n- \n", 7, false), EnterAction::Split { before: "- a".into(), after: "".into() });
        assert_eq!(on_enter("> q\n", 4, false), EnterAction::Rewrite { text: "> q\n> ".into(), cursor: 6 });
        assert_eq!(on_enter("> q\n> \n", 7, false), EnterAction::Split { before: "> q".into(), after: "".into() });
        assert_eq!(on_enter("```\nx\n", 6, false), EnterAction::Keep);
        assert_eq!(on_enter("- [x] a\n", 8, false), EnterAction::Rewrite { text: "- [x] a\n- [ ] ".into(), cursor: 14 });
    }

    #[test]
    fn finding() {
        assert_eq!(find_all("Abc abc ABC", "abc", false).len(), 3);
        assert_eq!(find_all("Abc abc ABC", "abc", true), vec![4..7]);
        let t = "Café BAR café";
        let m = find_all(t, "CAFÉ", false);
        assert_eq!(m.len(), 2);
        assert_eq!(&t[m[1].clone()], "café");
        assert_eq!(para_bounds("a\n\nb c\nd\n\ne", 4), 3..8);
    }
}
