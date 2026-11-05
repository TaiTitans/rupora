//! Document model: a Markdown file split into top-level blocks.
//!
//! The split is *lossless*: `Document::parse(s).to_markdown() == s` for every
//! input, because each block remembers the exact whitespace that followed it.
//! This lets the WYSIWYG editor work block-by-block (like Typora) without
//! reformatting the user's file on save.

use pulldown_cmark::{Event, Options, Parser};
use std::ops::Range;

/// Markdown extensions enabled everywhere in the app.
pub fn md_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_MATH
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// Raw markdown of the block (no trailing newline).
    pub text: String,
    /// Whitespace that follows the block (usually "\n\n", "\n" at EOF).
    pub sep: String,
}

impl Block {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), sep: "\n\n".into() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// Leading whitespace before the first block.
    pub prefix: String,
    pub blocks: Vec<Block>,
}

impl Default for Document {
    fn default() -> Self {
        Self { prefix: String::new(), blocks: vec![Block { text: String::new(), sep: "\n".into() }] }
    }
}

/// Byte ranges of top-level markdown blocks, extended to whole lines.
pub fn block_ranges(src: &str) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut depth = 0usize;
    let mut cur_start = 0usize;
    for (ev, range) in Parser::new_ext(src, md_options()).into_offset_iter() {
        match ev {
            Event::Start(_) => {
                if depth == 0 {
                    cur_start = range.start;
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    ranges.push(cur_start..range.end.max(cur_start));
                }
            }
            _ if depth == 0 => ranges.push(range),
            _ => {}
        }
    }

    // Extend each range to full lines and make the list monotonic.
    let bytes = src.as_bytes();
    let mut out: Vec<Range<usize>> = Vec::new();
    for r in ranges {
        let mut s = r.start.min(src.len());
        while s > 0 && bytes[s - 1] != b'\n' {
            s -= 1;
        }
        let mut e = r.end.min(src.len()).max(s);
        // Trim trailing newlines that the parser included, then extend to EOL.
        while e > s && (bytes[e - 1] == b'\n' || bytes[e - 1] == b'\r') {
            e -= 1;
        }
        while e < src.len() && bytes[e] != b'\n' {
            e += 1;
        }
        if e > s && bytes[e - 1] == b'\r' {
            e -= 1;
        }
        if let Some(last) = out.last_mut() {
            if s < last.end {
                // Overlap (rare: two blocks on one line) – merge.
                last.end = last.end.max(e);
                continue;
            }
        }
        if e > s {
            out.push(s..e);
        }
    }
    out
}

impl Document {
    pub fn parse(src: &str) -> Self {
        let ranges = block_ranges(src);
        // Any non-whitespace text not covered by a range (e.g. link reference
        // definitions) becomes its own block so nothing is ever lost.
        let mut cuts: Vec<Range<usize>> = Vec::new();
        let mut pos = 0usize;
        for r in ranges {
            if r.start > pos {
                push_gap_blocks(src, pos..r.start, &mut cuts);
            }
            cuts.push(r.clone());
            pos = r.end;
        }
        if pos < src.len() {
            push_gap_blocks(src, pos..src.len(), &mut cuts);
        }

        if cuts.is_empty() {
            return Self { prefix: String::new(), blocks: vec![Block { text: String::new(), sep: src.to_string() }] };
        }

        let prefix = src[..cuts[0].start].to_string();
        let mut blocks = Vec::with_capacity(cuts.len());
        for (i, r) in cuts.iter().enumerate() {
            let next = cuts.get(i + 1).map(|n| n.start).unwrap_or(src.len());
            blocks.push(Block { text: src[r.clone()].to_string(), sep: src[r.end..next].to_string() });
        }
        Self { prefix, blocks }
    }

    pub fn to_markdown(&self) -> String {
        let mut s = String::with_capacity(self.blocks.iter().map(|b| b.text.len() + b.sep.len()).sum::<usize>() + 8);
        s.push_str(&self.prefix);
        for b in &self.blocks {
            s.push_str(&b.text);
            s.push_str(&b.sep);
        }
        s
    }

    /// Byte offset in `to_markdown()` where each block's text starts.
    pub fn block_offsets(&self) -> Vec<usize> {
        let mut v = Vec::with_capacity(self.blocks.len());
        let mut pos = self.prefix.len();
        for b in &self.blocks {
            v.push(pos);
            pos += b.text.len() + b.sep.len();
        }
        v
    }

    /// Re-split block `i` if editing turned it into several blocks.
    /// Returns how many blocks it became.
    pub fn resplit(&mut self, i: usize) -> usize {
        if i >= self.blocks.len() {
            return 0;
        }
        let b = self.blocks[i].clone();
        let sub = Document::parse(&b.text);
        if sub.blocks.len() <= 1 && sub.prefix.is_empty() {
            return 1;
        }
        let mut new_blocks = sub.blocks;
        if new_blocks.is_empty() {
            return 1;
        }
        // Leading blank lines inside the block are attached to the previous block.
        if !sub.prefix.is_empty() {
            if i > 0 {
                self.blocks[i - 1].sep.push_str(&sub.prefix);
            } else {
                self.prefix.push_str(&sub.prefix);
            }
        }
        let last = new_blocks.len() - 1;
        let trailing = std::mem::take(&mut new_blocks[last].sep);
        new_blocks[last].sep = trailing + &b.sep;
        let n = new_blocks.len();
        self.blocks.splice(i..=i, new_blocks);
        n
    }

    pub fn heading_level(text: &str) -> Option<u8> {
        let first = text.lines().next()?;
        let t = first.trim_start();
        if first.len() - t.len() <= 3 && t.starts_with('#') {
            let n = t.chars().take_while(|c| *c == '#').count();
            let rest = &t[n..];
            if (1..=6).contains(&n) && (rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')) {
                return Some(n as u8);
            }
        }
        let mut lines = text.lines();
        let _ = lines.next();
        if let Some(second) = lines.next() {
            let s = second.trim();
            if !s.is_empty() && lines.next().is_none() {
                if s.chars().all(|c| c == '=') {
                    return Some(1);
                }
                if s.chars().all(|c| c == '-') && !first.trim().is_empty() {
                    return Some(2);
                }
            }
        }
        None
    }

    pub fn heading_text(text: &str) -> String {
        let first = text.lines().next().unwrap_or("").trim();
        let t = first.trim_start_matches('#').trim();
        let t = t.trim_end_matches('#').trim_end();
        strip_inline_markup(t)
    }
}

fn push_gap_blocks(src: &str, gap: Range<usize>, out: &mut Vec<Range<usize>>) {
    // Split gap into paragraphs of non-blank lines.
    let text = &src[gap.clone()];
    let mut line_start = gap.start;
    let mut cur: Option<Range<usize>> = None;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        let ls = line_start;
        let le = ls + content.len();
        if content.trim().is_empty() {
            if let Some(c) = cur.take() {
                out.push(c);
            }
        } else {
            match &mut cur {
                Some(c) => c.end = le,
                None => cur = Some(ls..le),
            }
        }
        line_start += line.len();
    }
    if let Some(c) = cur {
        out.push(c);
    }
}

/// Remove the most common inline markdown markers (for outline / titles).
pub fn strip_inline_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '_' | '`' | '~' => {}
            '!' if chars.peek() == Some(&'[') => {}
            '[' => {}
            ']' => {
                if chars.peek() == Some(&'(') {
                    for c2 in chars.by_ref() {
                        if c2 == ')' {
                            break;
                        }
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// Simple statistics for the status bar.
pub struct Stats {
    pub words: usize,
    pub chars: usize,
    pub lines: usize,
}

pub fn stats(text: &str) -> Stats {
    let mut words = 0;
    let mut in_word = false;
    let mut chars = 0;
    for c in text.chars() {
        if !c.is_whitespace() {
            chars += 1;
        }
        // CJK characters count as one word each.
        let cjk = matches!(c as u32, 0x4E00..=0x9FFF | 0x3040..=0x30FF | 0xAC00..=0xD7AF);
        if cjk {
            words += 1;
            in_word = false;
        } else if c.is_alphanumeric() {
            if !in_word {
                words += 1;
                in_word = true;
            }
        } else if !matches!(c, '\'' | '-' | '’') {
            in_word = false;
        }
    }
    let lines = if text.is_empty() { 0 } else { text.lines().count() };
    Stats { words, chars, lines }
}

/// Convert a char index to a byte index.
pub fn char_to_byte(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(s.len())
}

/// Convert a byte index to a char index.
pub fn byte_to_char(s: &str, bi: usize) -> usize {
    s[..bi.min(s.len())].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLES: &[&str] = &[
        "",
        "\n\n",
        "hello",
        "hello\n",
        "# Title\n\nPara one\nstill para\n\n- a\n- b\n\n  - nested\n\n```rust\nfn main() {}\n\n\n}\n```\n\n> quote\n> more\n\n---\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
        "\n\n\n# Lead blank\n\n\n\ntext\n\n\n",
        "para\n[ref]: http://example.com\n\n[ref2]: http://x.y\n\nafter",
        "---\ntitle: front\n---\n\n# H\n",
        "Setext\n======\n\nSub\n---\n",
        "1. one\n2. two\n\n   para in item\n3. three\n",
        "- [ ] todo\n- [x] done\n",
        "Line with CRLF\r\n\r\nNext\r\n",
        "$$\nx^2\n$$\n\ntext $a$ end\n",
        "    indented code\n\n    more\n\nafter\n",
        "Accented café, naïve\n\n## Résumé\n",
        "<div>\nhtml\n</div>\n\npara",
        "a[^1]\n\n[^1]: footnote\n",
    ];

    #[test]
    fn roundtrip_is_lossless() {
        for s in SAMPLES {
            let d = Document::parse(s);
            assert_eq!(&d.to_markdown(), s, "roundtrip failed for {s:?}\n{d:#?}");
            assert!(!d.blocks.is_empty());
        }
    }

    #[test]
    fn splits_into_expected_blocks() {
        let d = Document::parse(SAMPLES[4]);
        let texts: Vec<_> = d.blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(texts[0], "# Title");
        assert_eq!(texts[1], "Para one\nstill para");
        assert!(texts[2].starts_with("- a\n- b"));
        assert!(texts[3].starts_with("```rust") && texts[3].ends_with("```"));
        assert_eq!(texts[4], "> quote\n> more");
        assert_eq!(texts[5], "---");
        assert!(texts[6].starts_with("| a"));
        assert_eq!(texts.len(), 7);
    }

    #[test]
    fn link_refs_are_kept() {
        let d = Document::parse(SAMPLES[6]);
        assert!(d.blocks.iter().any(|b| b.text.contains("[ref2]: http://x.y")));
    }

    #[test]
    fn resplit_block() {
        let mut d = Document::parse("one\n\ntwo\n");
        d.blocks[0].text = "one\n\ninserted".into();
        assert_eq!(d.resplit(0), 2);
        assert_eq!(d.to_markdown(), "one\n\ninserted\n\ntwo\n");
        assert_eq!(d.blocks.len(), 3);
    }

    #[test]
    fn headings() {
        assert_eq!(Document::heading_level("## Hi"), Some(2));
        assert_eq!(Document::heading_level("#nospace"), None);
        assert_eq!(Document::heading_level("Title\n==="), Some(1));
        assert_eq!(Document::heading_text("## **Bold** `code` [l](x) ##"), "Bold code l");
    }

    #[test]
    fn word_count() {
        let s = stats("Hello to the café! hello-world 漢字");
        assert_eq!(s.words, 7);
    }

    #[test]
    fn offsets() {
        let d = Document::parse("\na\n\nbb\n");
        assert_eq!(d.block_offsets(), vec![1, 4]);
    }
}
