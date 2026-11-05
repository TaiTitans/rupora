//! Renders one markdown block into egui widgets (the "preview" half of the
//! Typora-style live editor).

use crate::document::md_options;
use crate::fonts::{bold, mono, regular, serif, Sizes};
use crate::highlight::{code_job, heading_scale};
use crate::theme::Theme;
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Color32, CornerRadius, FontId, Label, Margin, Rect, Stroke, Vec2};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Parser, Tag};
use std::ops::Range;
use std::path::Path;

// ───────────────────────────── AST ─────────────────────────────

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct St {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub math: bool,
    pub html: bool,
    pub footnote: bool,
    pub link: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text { text: String, st: St },
    Image { alt: String, url: String },
    Break,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// (checked, byte offset of the `[ ]` marker inside the block)
    pub task: Option<(bool, usize)>,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Para(Vec<Inline>),
    Heading(u8, Vec<Inline>),
    Quote(Vec<Node>),
    Code { lang: String, text: String },
    List { start: Option<u64>, items: Vec<Item> },
    Rule,
    Table { aligns: Vec<Alignment>, rows: Vec<Vec<Vec<Inline>>> },
    Html(String),
    Meta(String),
    Footnote(String, Vec<Node>),
}

struct P<'a> {
    evs: Vec<(Event<'a>, Range<usize>)>,
    pos: usize,
    task: Option<(bool, usize)>,
    links: Vec<String>,
}

impl<'a> P<'a> {
    fn next(&mut self) -> Option<(Event<'a>, Range<usize>)> {
        let e = self.evs.get(self.pos).cloned();
        self.pos += 1;
        e
    }
}

#[derive(Default)]
struct Col {
    out: Vec<Inline>,
    stack: Vec<Frame>,
    alt: Option<(String, String)>,
}

enum Frame {
    Em,
    Strong,
    Strike,
    Link,
    Image,
    Other,
}

impl Col {
    fn st(&self) -> St {
        let mut s = St::default();
        for f in &self.stack {
            match f {
                Frame::Em => s.italic = true,
                Frame::Strong => s.bold = true,
                Frame::Strike => s.strike = true,
                Frame::Link => s.link = true,
                _ => {}
            }
        }
        s
    }
    fn text(&mut self, t: &str, f: impl FnOnce(&mut St)) {
        if let Some((_, alt)) = &mut self.alt {
            alt.push_str(t);
            return;
        }
        let mut st = self.st();
        f(&mut st);
        self.out.push(Inline::Text { text: t.to_string(), st });
    }
    fn feed(&mut self, ev: Event, links: &mut Vec<String>) {
        match ev {
            Event::Start(tag) => self.stack.push(match tag {
                Tag::Emphasis => Frame::Em,
                Tag::Strong => Frame::Strong,
                Tag::Strikethrough => Frame::Strike,
                Tag::Link { dest_url, .. } => {
                    links.push(dest_url.to_string());
                    Frame::Link
                }
                Tag::Image { dest_url, .. } => {
                    self.alt = Some((dest_url.to_string(), String::new()));
                    Frame::Image
                }
                _ => Frame::Other,
            }),
            Event::End(_) => {
                if let Some(Frame::Image) = self.stack.pop() {
                    if let Some((url, alt)) = self.alt.take() {
                        self.out.push(Inline::Image { alt, url });
                    }
                }
            }
            Event::Text(t) => self.text(&t, |_| {}),
            Event::Code(t) => self.text(&t, |s| s.code = true),
            Event::InlineMath(t) => self.text(&t, |s| s.math = true),
            Event::DisplayMath(t) => {
                self.out.push(Inline::Break);
                self.text(t.trim(), |s| s.math = true);
                self.out.push(Inline::Break);
            }
            Event::SoftBreak => self.text(" ", |_| {}),
            Event::HardBreak => self.out.push(Inline::Break),
            Event::InlineHtml(h) | Event::Html(h) => {
                let l = h.trim().to_ascii_lowercase();
                if l == "<br>" || l == "<br/>" || l == "<br />" {
                    self.out.push(Inline::Break);
                } else {
                    self.text(&h, |s| s.html = true);
                }
            }
            Event::FootnoteReference(l) => self.text(&format!("[{l}]"), |s| s.footnote = true),
            _ => {}
        }
    }
}

fn is_inline_start(tag: &Tag) -> bool {
    matches!(tag, Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. })
}

fn parse_inlines(p: &mut P) -> Vec<Inline> {
    let mut col = Col::default();
    while let Some((ev, r)) = p.next() {
        match ev {
            Event::End(_) if col.stack.is_empty() => break,
            Event::TaskListMarker(b) => p.task = Some((b, r.start)),
            ev => col.feed(ev, &mut p.links),
        }
    }
    col.out
}

fn collect_text(p: &mut P) -> String {
    let mut depth = 0;
    let mut s = String::new();
    while let Some((ev, _)) = p.next() {
        match ev {
            Event::Start(_) => depth += 1,
            Event::End(_) if depth == 0 => break,
            Event::End(_) => depth -= 1,
            Event::Text(t) | Event::Code(t) | Event::Html(t) | Event::InlineHtml(t) | Event::InlineMath(t) | Event::DisplayMath(t) => {
                s.push_str(&t)
            }
            Event::SoftBreak | Event::HardBreak => s.push('\n'),
            _ => {}
        }
    }
    s
}

fn parse_blocks(p: &mut P) -> Vec<Node> {
    let mut v = Vec::new();
    while let Some((ev, r)) = p.next() {
        match ev {
            Event::End(_) => break,
            ev => {
                if let Some(n) = parse_block(p, ev, r) {
                    v.push(n);
                }
            }
        }
    }
    v
}

fn parse_item(p: &mut P) -> Item {
    let saved = p.task.take();
    let mut children = Vec::new();
    let mut col = Col::default();
    let flush = |col: &mut Col, children: &mut Vec<Node>| {
        if !col.out.is_empty() {
            children.push(Node::Para(std::mem::take(&mut col.out)));
        }
    };
    while let Some((ev, r)) = p.next() {
        let block_start = matches!(&ev, Event::Start(t) if !is_inline_start(t));
        if block_start {
            flush(&mut col, &mut children);
            if let Some(n) = parse_block(p, ev, r) {
                children.push(n);
            }
            continue;
        }
        match ev {
            Event::End(_) if col.stack.is_empty() => break,
            Event::TaskListMarker(b) => p.task = Some((b, r.start)),
            Event::Rule => {
                flush(&mut col, &mut children);
                children.push(Node::Rule);
            }
            ev => col.feed(ev, &mut p.links),
        }
    }
    flush(&mut col, &mut children);
    let task = p.task.take();
    p.task = saved;
    Item { task, children }
}

fn parse_block<'a>(p: &mut P<'a>, ev: Event<'a>, _r: Range<usize>) -> Option<Node> {
    Some(match ev {
        Event::Start(tag) => match tag {
            Tag::Paragraph => Node::Para(parse_inlines(p)),
            Tag::Heading { level, .. } => Node::Heading(level as u8, parse_inlines(p)),
            Tag::BlockQuote(_) => Node::Quote(parse_blocks(p)),
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => {
                        l.split(|c: char| c.is_whitespace() || c == ',' || c == '{').next().unwrap_or("").to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                let mut text = collect_text(p);
                if text.ends_with('\n') {
                    text.pop();
                }
                Node::Code { lang, text }
            }
            Tag::List(start) => {
                let mut items = Vec::new();
                while let Some((ev, _)) = p.next() {
                    match ev {
                        Event::Start(Tag::Item) => items.push(parse_item(p)),
                        Event::End(_) => break,
                        _ => {}
                    }
                }
                Node::List { start, items }
            }
            Tag::Table(aligns) => {
                let mut rows = Vec::new();
                while let Some((ev, _)) = p.next() {
                    match ev {
                        Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {
                            let mut cells = Vec::new();
                            while let Some((ev, _)) = p.next() {
                                match ev {
                                    Event::Start(Tag::TableCell) => cells.push(parse_inlines(p)),
                                    Event::End(_) => break,
                                    _ => {}
                                }
                            }
                            rows.push(cells);
                        }
                        Event::End(_) => break,
                        _ => {}
                    }
                }
                Node::Table { aligns, rows }
            }
            Tag::HtmlBlock => Node::Html(collect_text(p).trim_end().to_string()),
            Tag::MetadataBlock(_) => Node::Meta(collect_text(p).trim_end().to_string()),
            Tag::FootnoteDefinition(l) => Node::Footnote(l.to_string(), parse_blocks(p)),
            Tag::Item => Node::List { start: None, items: vec![parse_item(p)] },
            _ => Node::Para(vec![Inline::Text { text: collect_text(p), st: St::default() }]),
        },
        Event::Rule => Node::Rule,
        Event::Html(h) => Node::Html(h.to_string()),
        Event::Text(t) => Node::Para(vec![Inline::Text { text: t.to_string(), st: St::default() }]),
        _ => return None,
    })
}

/// Parse a markdown block into an AST. Also returns all link targets.
pub fn parse(text: &str) -> (Vec<Node>, Vec<String>) {
    let evs: Vec<_> = Parser::new_ext(text, md_options()).into_offset_iter().collect();
    let mut p = P { evs, pos: 0, task: None, links: Vec::new() };
    let mut nodes = Vec::new();
    while p.pos < p.evs.len() {
        nodes.extend(parse_blocks(&mut p));
    }
    (nodes, p.links)
}

// ─────────────────────────── rendering ───────────────────────────

#[derive(Clone)]
pub struct Rc<'a> {
    pub theme: &'a Theme,
    pub sizes: Sizes,
    pub base_dir: Option<&'a Path>,
    pub color: Color32,
}

impl Rc<'_> {
    fn body(&self, size: f32) -> FontId {
        if self.theme.serif {
            serif(size)
        } else {
            regular(size)
        }
    }
}

#[derive(Default, Debug)]
pub struct RenderOut {
    /// Byte offset of a task marker (`[ ]`) the user toggled.
    pub toggle_task: Option<usize>,
    pub links: Vec<String>,
}

fn inline_job(items: &[Inline], rc: &Rc, size: f32, color: Color32, heading: bool, width: f32) -> (LayoutJob, Vec<(String, String)>) {
    let t = rc.theme;
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    let mut images = Vec::new();
    for it in items {
        match it {
            Inline::Break => job.append("\n", 0.0, TextFormat { font_id: rc.body(size), color, ..Default::default() }),
            Inline::Image { alt, url } => images.push((alt.clone(), url.clone())),
            Inline::Text { text, st } => {
                let mut f = TextFormat::default();
                if st.code || st.math || st.html {
                    f.font_id = mono(size * 0.88);
                    f.color = if st.math {
                        t.accent
                    } else if st.html {
                        t.muted
                    } else {
                        t.code_text
                    };
                    if st.code {
                        f.background = t.code_bg;
                    }
                    f.italics = st.math;
                } else if st.footnote {
                    f.font_id = regular(size * 0.72);
                    f.color = t.accent;
                    f.valign = Align::TOP;
                } else {
                    f.font_id = if st.bold || heading { bold(size) } else { rc.body(size) };
                    f.color = if st.link {
                        t.link
                    } else if st.bold && !heading {
                        t.heading
                    } else {
                        color
                    };
                    f.italics = st.italic;
                }
                if st.strike {
                    f.strikethrough = Stroke::new(1.2, f.color);
                }
                if st.link {
                    f.color = t.link;
                    f.underline = Stroke::new(1.0, t.link.gamma_multiply(0.6));
                }
                job.append(text, 0.0, f);
            }
        }
    }
    (job, images)
}

fn show_images(ui: &mut egui::Ui, rc: &Rc, images: &[(String, String)]) {
    for (alt, url) in images {
        let label = if alt.is_empty() { url.as_str() } else { alt.as_str() };
        let is_remote = url.starts_with("http://") || url.starts_with("https://");
        let path = if is_remote {
            None
        } else {
            let p = Path::new(url.trim_start_matches("file://"));
            let p = if p.is_absolute() { p.to_path_buf() } else { rc.base_dir.map(|d| d.join(p)).unwrap_or_else(|| p.to_path_buf()) };
            p.exists().then_some(p)
        };
        match path {
            Some(p) => {
                let max_w = ui.available_width();
                ui.add(egui::Image::new(format!("file://{}", p.display())).max_width(max_w).max_height(600.0).corner_radius(4))
                    .on_hover_text(label);
            }
            None => {
                let mut job = LayoutJob::default();
                job.append(
                    &format!("🖼 {label}"),
                    0.0,
                    TextFormat { font_id: regular(rc.sizes.small), color: rc.theme.muted, italics: true, ..Default::default() },
                );
                ui.label(job).on_hover_text(url.as_str());
            }
        }
    }
}

fn paragraph(ui: &mut egui::Ui, items: &[Inline], rc: &Rc, size: f32, color: Color32, heading: bool) {
    let w = ui.available_width();
    let (job, images) = inline_job(items, rc, size, color, heading, w);
    if !job.text.trim().is_empty() {
        ui.add(Label::new(job).wrap());
    }
    show_images(ui, rc, &images);
}

fn render_nodes(ui: &mut egui::Ui, nodes: &[Node], rc: &Rc, depth: usize, out: &mut RenderOut) {
    for (idx, n) in nodes.iter().enumerate() {
        ui.push_id(idx, |ui| render_node(ui, n, rc, depth, out));
    }
}

fn render_node(ui: &mut egui::Ui, node: &Node, rc: &Rc, depth: usize, out: &mut RenderOut) {
    let t = rc.theme;
    let sz = rc.sizes;
    match node {
        Node::Para(items) => paragraph(ui, items, rc, sz.body, rc.color, false),
        Node::Heading(level, items) => {
            let size = sz.body * heading_scale(*level);
            let color = if *level == 6 { t.muted } else { t.heading };
            ui.add_space(if *level <= 2 { 6.0 } else { 2.0 });
            paragraph(ui, items, rc, size, color, true);
            if *level <= 2 {
                let r = ui.available_rect_before_wrap();
                let y = ui.min_rect().bottom() + 2.0;
                ui.painter().hline(r.left()..=r.right(), y, Stroke::new(1.0, t.rule));
                ui.add_space(6.0);
            }
        }
        Node::Quote(children) => {
            let rc2 = Rc { color: t.quote_text, ..rc.clone() };
            let resp = egui::Frame::NONE.inner_margin(Margin { left: 16, right: 4, top: 2, bottom: 2 }).show(ui, |ui| {
                ui.set_width(ui.available_width());
                render_nodes(ui, children, &rc2, depth, out);
            });
            let r = resp.response.rect;
            ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(4.0, r.height())), CornerRadius::same(1), t.quote_bar);
        }
        Node::Code { lang, text } => {
            let resp = egui::Frame::NONE.fill(t.code_bg).corner_radius(CornerRadius::same(4)).inner_margin(Margin::symmetric(12, 10)).show(
                ui,
                |ui| {
                    let w = ui.available_width();
                    ui.set_width(w);
                    let job = code_job(lang, text, t, sz.mono, w);
                    ui.add(Label::new(job).wrap());
                },
            );
            if !lang.is_empty() {
                let r = resp.response.rect;
                ui.painter().text(r.right_top() + Vec2::new(-8.0, 4.0), egui::Align2::RIGHT_TOP, lang, regular(sz.small * 0.85), t.muted);
            }
        }
        Node::List { start, items } => {
            for (i, item) in items.iter().enumerate() {
                ui.push_id(i, |ui| {
                    ui.horizontal_top(|ui| {
                        let marker_w = sz.body * 1.5;
                        let row_h = sz.body * 1.3;
                        if let Some((checked, off)) = item.task {
                            let mut c = checked;
                            ui.allocate_ui_with_layout(Vec2::new(marker_w, row_h), egui::Layout::right_to_left(Align::Center), |ui| {
                                if ui.checkbox(&mut c, "").changed() {
                                    out.toggle_task = Some(off);
                                }
                            });
                        } else {
                            let marker = match start {
                                Some(s) => format!("{}.", s + i as u64),
                                None => if depth % 2 == 0 { "•" } else { "◦" }.to_string(),
                            };
                            let font = if start.is_some() { rc.body(sz.body) } else { bold(sz.body) };
                            ui.allocate_ui_with_layout(Vec2::new(marker_w, row_h), egui::Layout::right_to_left(Align::Min), |ui| {
                                ui.label(egui::RichText::new(marker).font(font).color(if start.is_some() { rc.color } else { t.muted }));
                            });
                        }
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 3.0;
                            render_nodes(ui, &item.children, rc, depth + 1, out);
                        });
                    });
                });
            }
        }
        Node::Rule => {
            ui.add_space(6.0);
            let r = ui.available_rect_before_wrap();
            let (rect, _) = ui.allocate_exact_size(Vec2::new(r.width(), 2.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::ZERO, t.rule);
            ui.add_space(6.0);
        }
        Node::Table { aligns, rows } => {
            egui::Frame::NONE.stroke(Stroke::new(1.0, t.table_border)).inner_margin(Margin::same(6)).show(ui, |ui| {
                egui::Grid::new("table").striped(true).spacing(Vec2::new(18.0, 6.0)).show(ui, |ui| {
                    for (ri, row) in rows.iter().enumerate() {
                        for (ci, cell) in row.iter().enumerate() {
                            let (job, _) =
                                inline_job(cell, rc, sz.body, if ri == 0 { t.heading } else { rc.color }, ri == 0, f32::INFINITY);
                            let align = match aligns.get(ci) {
                                Some(Alignment::Center) => egui::Layout::top_down(Align::Center),
                                Some(Alignment::Right) => egui::Layout::top_down(Align::Max),
                                _ => egui::Layout::top_down(Align::Min),
                            };
                            ui.with_layout(align, |ui| ui.label(job));
                        }
                        ui.end_row();
                    }
                });
            });
        }
        Node::Html(h) => {
            ui.label(egui::RichText::new(h).font(mono(sz.mono)).color(t.muted));
        }
        Node::Meta(m) => {
            egui::Frame::NONE.fill(t.code_bg).corner_radius(CornerRadius::same(4)).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(egui::RichText::new(m).font(mono(sz.mono)).color(t.muted));
            });
        }
        Node::Footnote(label, children) => {
            ui.horizontal_top(|ui| {
                ui.label(egui::RichText::new(format!("[{label}]")).font(regular(sz.small)).color(t.accent));
                ui.vertical(|ui| {
                    let rc2 = Rc { sizes: Sizes::new(sz.small), ..rc.clone() };
                    render_nodes(ui, children, &rc2, depth, out);
                });
            });
        }
    }
}

/// Render a whole block. `text` is the block's raw markdown.
pub fn render_block(ui: &mut egui::Ui, text: &str, rc: &Rc) -> RenderOut {
    let (nodes, links) = parse(text);
    let mut out = RenderOut { links, ..Default::default() };
    render_nodes(ui, &nodes, rc, 0, &mut out);
    out
}

/// Toggle a task list checkbox at byte offset `off` (pointing at `[`).
pub fn toggle_task(text: &mut String, off: usize) {
    if off + 2 < text.len() && text.as_bytes()[off] == b'[' {
        let c = text.as_bytes()[off + 1];
        let rep = if c == b' ' { "x" } else { " " };
        text.replace_range(off + 1..off + 2, rep);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tasks_with_offsets() {
        let src = "- [ ] one\n- [x] two\n  - [ ] nested\n";
        let (nodes, _) = parse(src);
        let Node::List { items, .. } = &nodes[0] else { panic!("{nodes:?}") };
        let (c0, o0) = items[0].task.unwrap();
        assert!(!c0);
        assert_eq!(&src[o0..o0 + 3], "[ ]");
        let (c1, o1) = items[1].task.unwrap();
        assert!(c1);
        assert_eq!(&src[o1..o1 + 3], "[x]");
        let Node::List { items: inner, .. } = &items[1].children[1] else { panic!("{:?}", items[1]) };
        let (_, o2) = inner[0].task.unwrap();
        assert_eq!(&src[o2..o2 + 3], "[ ]");
        let mut s = src.to_string();
        toggle_task(&mut s, o0);
        assert!(s.starts_with("- [x] one"));
    }

    #[test]
    fn loose_list_tasks() {
        let src = "- [ ] a\n\n- [x] b\n";
        let (nodes, _) = parse(src);
        let Node::List { items, .. } = &nodes[0] else { panic!() };
        assert_eq!(items[0].task.map(|t| t.0), Some(false));
        assert_eq!(items[1].task.map(|t| t.0), Some(true));
    }

    #[test]
    fn parses_various() {
        let (n, links) = parse("# T *i* [l](http://a)\n");
        assert!(matches!(&n[0], Node::Heading(1, _)));
        assert_eq!(links, vec!["http://a".to_string()]);
        let (n, _) = parse("| a | b |\n|:-|-:|\n| 1 | 2 |\n");
        let Node::Table { rows, aligns } = &n[0] else { panic!() };
        assert_eq!(rows.len(), 2);
        assert_eq!(aligns, &vec![Alignment::Left, Alignment::Right]);
        let (n, _) = parse("```rust\nfn x(){}\n```");
        assert_eq!(n[0], Node::Code { lang: "rust".into(), text: "fn x(){}".into() });
        let (n, _) = parse("![alt](img.png)");
        let Node::Para(items) = &n[0] else { panic!() };
        assert_eq!(items[0], Inline::Image { alt: "alt".into(), url: "img.png".into() });
        let (n, _) = parse("> q\n>\n> - x");
        assert!(matches!(&n[0], Node::Quote(c) if c.len() == 2));
    }
}
