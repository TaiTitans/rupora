//! The application: window chrome, menus, sidebar, the block-based live
//! editor, source mode, search/replace, file handling and settings.

use crate::document::{byte_to_char, char_to_byte, stats, Block, Document};
use crate::edit::{self, EnterAction, Kind, ListKind};
use crate::files::{self, FileNode};
use crate::fonts::{self, Sizes};
use crate::highlight::markdown_job;
use crate::render::{self, Rc};
use crate::theme::{Theme, ThemeKind};
use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{Align, Align2, Context, CursorIcon, Id, Key, KeyboardShortcut, Modifiers, Rect, RichText, Sense, Vec2, ViewportCommand};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const WELCOME: &str = include_str!("../assets/welcome.md");
const SETTINGS_KEY: &str = "rupora-settings";

// ───────────────────────────── settings ─────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeKind,
    pub font_size: f32,
    pub content_width: f32,
    pub show_sidebar: bool,
    pub show_status: bool,
    pub focus_mode: bool,
    pub typewriter: bool,
    pub autosave: bool,
    pub recent: Vec<PathBuf>,
    pub folder: Option<PathBuf>,
    pub welcome_shown: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeKind::Github,
            font_size: 16.0,
            content_width: 860.0,
            show_sidebar: false,
            show_status: true,
            focus_mode: false,
            typewriter: false,
            autosave: false,
            recent: Vec::new(),
            folder: None,
            welcome_shown: false,
        }
    }
}

// ───────────────────────────── actions ─────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum Action {
    New,
    Open,
    OpenFolder,
    CloseFolder,
    OpenPath(PathBuf),
    Save,
    SaveAs,
    ExportHtml,
    Quit,
    Undo,
    Redo,
    Find,
    Replace,
    ToggleSource,
    ToggleSidebar,
    ToggleStatus,
    Focus,
    Typewriter,
    Autosave,
    Heading(u8),
    Quote,
    List(ListKind),
    CodeFence,
    MathBlock,
    Table,
    Rule,
    Inline(&'static str),
    Link,
    Theme(ThemeKind),
    FontBigger,
    FontSmaller,
    FontReset,
    Wider,
    Narrower,
    About,
    Shortcuts,
}

#[derive(Clone, Copy)]
struct Sc {
    sc: KeyboardShortcut,
}

const fn sc(m: Modifiers, k: Key) -> Sc {
    Sc { sc: KeyboardShortcut::new(m, k) }
}
const CMD: Modifiers = Modifiers::COMMAND;
const CMD_SHIFT: Modifiers = Modifiers { shift: true, ..Modifiers::COMMAND };
const ALT_SHIFT: Modifiers = Modifiers { alt: true, shift: true, ..Modifiers::NONE };

/// Ordered so that more specific shortcuts (with Shift) are checked first.
fn shortcut_table() -> Vec<(Sc, Action)> {
    vec![
        (sc(CMD_SHIFT, Key::S), Action::SaveAs),
        (sc(CMD_SHIFT, Key::O), Action::OpenFolder),
        (sc(CMD_SHIFT, Key::E), Action::ExportHtml),
        (sc(CMD_SHIFT, Key::Z), Action::Redo),
        (sc(CMD_SHIFT, Key::L), Action::ToggleSidebar),
        (sc(CMD_SHIFT, Key::Q), Action::Quote),
        (sc(CMD_SHIFT, Key::OpenBracket), Action::List(ListKind::Ordered)),
        (sc(CMD_SHIFT, Key::CloseBracket), Action::List(ListKind::Bullet)),
        (sc(CMD_SHIFT, Key::X), Action::List(ListKind::Task)),
        (sc(CMD_SHIFT, Key::K), Action::CodeFence),
        (sc(CMD_SHIFT, Key::M), Action::MathBlock),
        (sc(CMD_SHIFT, Key::Backtick), Action::Inline("`")),
        (sc(ALT_SHIFT, Key::Num5), Action::Inline("~~")),
        (sc(CMD, Key::N), Action::New),
        (sc(CMD, Key::O), Action::Open),
        (sc(CMD, Key::S), Action::Save),
        (sc(CMD, Key::Z), Action::Undo),
        (sc(CMD, Key::Y), Action::Redo),
        (sc(CMD, Key::F), Action::Find),
        (sc(CMD, Key::H), Action::Replace),
        (sc(CMD, Key::Slash), Action::ToggleSource),
        (sc(CMD, Key::B), Action::Inline("**")),
        (sc(CMD, Key::I), Action::Inline("*")),
        (sc(CMD, Key::K), Action::Link),
        (sc(CMD, Key::T), Action::Table),
        (sc(CMD, Key::Num0), Action::Heading(0)),
        (sc(CMD, Key::Num1), Action::Heading(1)),
        (sc(CMD, Key::Num2), Action::Heading(2)),
        (sc(CMD, Key::Num3), Action::Heading(3)),
        (sc(CMD, Key::Num4), Action::Heading(4)),
        (sc(CMD, Key::Num5), Action::Heading(5)),
        (sc(CMD, Key::Num6), Action::Heading(6)),
        (sc(CMD, Key::Equals), Action::FontBigger),
        (sc(CMD, Key::Plus), Action::FontBigger),
        (sc(CMD, Key::Minus), Action::FontSmaller),
        (sc(Modifiers::NONE, Key::F8), Action::Focus),
        (sc(Modifiers::NONE, Key::F9), Action::Typewriter),
    ]
}

fn describe(a: &Action) -> String {
    match a {
        Action::New => "New file".into(),
        Action::Open => "Open file".into(),
        Action::OpenFolder => "Open folder".into(),
        Action::Save => "Save".into(),
        Action::SaveAs => "Save as".into(),
        Action::ExportHtml => "Export HTML".into(),
        Action::Undo => "Undo".into(),
        Action::Redo => "Redo".into(),
        Action::Find => "Find".into(),
        Action::Replace => "Replace".into(),
        Action::ToggleSource => "Source mode".into(),
        Action::ToggleSidebar => "Sidebar".into(),
        Action::Focus => "Focus mode".into(),
        Action::Typewriter => "Typewriter mode".into(),
        Action::Heading(0) => "Normal paragraph".into(),
        Action::Heading(l) => format!("Heading {l}"),
        Action::Quote => "Quote".into(),
        Action::List(ListKind::Ordered) => "Ordered list".into(),
        Action::List(ListKind::Bullet) => "Bullet list".into(),
        Action::List(ListKind::Task) => "Task list".into(),
        Action::CodeFence => "Code block".into(),
        Action::MathBlock => "Math block".into(),
        Action::Table => "Insert table".into(),
        Action::Inline("**") => "Bold".into(),
        Action::Inline("*") => "Italic".into(),
        Action::Inline("`") => "Inline code".into(),
        Action::Inline("~~") => "Strikethrough".into(),
        Action::Link => "Link".into(),
        Action::FontBigger => "Larger text".into(),
        Action::FontSmaller => "Smaller text".into(),
        other => format!("{other:?}"),
    }
}

fn shortcut_for(a: &Action) -> Option<KeyboardShortcut> {
    shortcut_table().into_iter().find(|(_, b)| b == a).map(|(s, _)| s.sc)
}

/// Lay out content in a centred column of at most `max_w` points.
fn column<R>(max_w: f32, ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let avail = ui.available_width();
    let w = avail.min(max_w).max(200.0);
    let pad = ((avail - w) / 2.0).max(0.0);
    ui.horizontal_top(|ui| {
        ui.add_space(pad);
        ui.vertical(|ui| {
            ui.set_width(w - 8.0);
            add(ui)
        })
        .inner
    })
    .inner
}

// ───────────────────────────── state ─────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum SideTab {
    Files,
    Outline,
}

#[derive(Clone, Debug)]
enum Pending {
    New,
    Open(Option<PathBuf>),
    Quit,
}

#[derive(Default)]
struct Find {
    open: bool,
    replace: bool,
    query: String,
    with: String,
    case: bool,
    focus: bool,
    last: Option<std::ops::Range<usize>>,
}

#[derive(Default)]
struct Undo {
    undo: Vec<String>,
    redo: Vec<String>,
    last: Option<Instant>,
    snapshot: String,
}

#[derive(Clone, Copy)]
enum Cur {
    Start,
    End,
    At(usize, usize),
}

pub struct App {
    s: Settings,
    theme: Theme,
    sizes: Sizes,
    doc: Document,
    source_mode: bool,
    source: String,
    active: Option<usize>,
    epoch: u64,
    pending_cursor: Option<(usize, usize)>,
    just_activated: bool,
    path: Option<PathBuf>,
    crlf: bool,
    saved_text: String,
    dirty: bool,
    undo: Undo,
    tree: Option<FileNode>,
    side_tab: SideTab,
    find: Find,
    scroll_to: Option<usize>,
    source_scroll_to: Option<usize>,
    source_galley: Option<(std::sync::Arc<egui::Galley>, egui::Pos2)>,
    pending: Option<Pending>,
    allow_close: bool,
    show_about: bool,
    show_shortcuts: bool,
    toast: Option<(String, Instant)>,
    block_rects: Vec<Rect>,
    last_change: Instant,
    title: String,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, open: Option<PathBuf>) -> Self {
        fonts::install(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
        let s: Settings = cc.storage.and_then(|st| eframe::get_value(st, SETTINGS_KEY)).unwrap_or_default();
        let theme = Theme::get(s.theme);
        theme.apply(&cc.egui_ctx);
        let mut app = Self {
            sizes: Sizes::new(s.font_size),
            theme,
            doc: Document::default(),
            source_mode: false,
            source: String::new(),
            active: None,
            epoch: 0,
            pending_cursor: None,
            just_activated: false,
            path: None,
            crlf: false,
            saved_text: String::new(),
            dirty: false,
            undo: Undo::default(),
            tree: None,
            side_tab: SideTab::Outline,
            find: Find::default(),
            scroll_to: None,
            source_scroll_to: None,
            source_galley: None,
            pending: None,
            allow_close: false,
            show_about: false,
            show_shortcuts: false,
            toast: None,
            block_rects: Vec::new(),
            last_change: Instant::now(),
            title: String::new(),
            s,
        };
        if let Some(folder) = app.s.folder.clone() {
            app.tree = files::scan(&folder);
            if app.tree.is_some() {
                app.side_tab = SideTab::Files;
            }
        }
        match open {
            Some(p) if p.is_dir() => app.open_folder_path(p),
            Some(p) => app.open_path(&p),
            None if !app.s.welcome_shown => {
                app.load_text(WELCOME.to_string(), None, false);
                app.saved_text = WELCOME.to_string();
                app.dirty = false;
                app.s.welcome_shown = true;
            }
            None => app.new_doc(),
        }
        app
    }

    // ───────────── text access / change tracking ─────────────

    fn current_text(&self) -> String {
        if self.source_mode {
            self.source.clone()
        } else {
            self.doc.to_markdown()
        }
    }

    fn changed(&mut self, force_group: bool) {
        let text = self.current_text();
        if text == self.undo.snapshot {
            return;
        }
        let now = Instant::now();
        let new_group = force_group || self.undo.last.is_none_or(|t| now.duration_since(t) > Duration::from_millis(900));
        let old = std::mem::replace(&mut self.undo.snapshot, text);
        if new_group {
            self.undo.undo.push(old);
            if self.undo.undo.len() > 300 {
                self.undo.undo.remove(0);
            }
        }
        self.undo.last = if force_group { None } else { Some(now) };
        self.undo.redo.clear();
        self.dirty = self.undo.snapshot != self.saved_text;
        self.last_change = now;
    }

    /// Replace the whole document text (keeps undo history if `keep_undo`).
    fn load_text(&mut self, text: String, path: Option<PathBuf>, keep_undo: bool) {
        if self.source_mode {
            self.source = text.clone();
        } else {
            self.doc = Document::parse(&text);
        }
        if !keep_undo {
            self.undo = Undo::default();
            self.path = path;
            self.active = None;
        } else if let Some(a) = self.active {
            self.active = Some(a.min(self.doc.blocks.len().saturating_sub(1)));
        }
        self.undo.snapshot = text;
        self.undo.last = None;
        self.epoch += 1;
        self.find.last = None;
        self.dirty = self.undo.snapshot != self.saved_text;
    }

    fn new_doc(&mut self) {
        self.load_text(String::new(), None, false);
        self.saved_text = String::new();
        self.crlf = false;
        self.dirty = false;
        if !self.source_mode {
            self.active = Some(0);
            self.pending_cursor = Some((0, 0));
        }
    }

    fn open_path(&mut self, p: &Path) {
        match files::read(p) {
            Ok(l) => {
                self.saved_text = l.text.clone();
                self.crlf = l.crlf;
                self.load_text(l.text, Some(p.to_path_buf()), false);
                self.dirty = false;
                self.add_recent(p.to_path_buf());
                self.scroll_to = Some(0);
            }
            Err(e) => self.notify(format!("Could not open file: {e}")),
        }
    }

    fn open_folder_path(&mut self, p: PathBuf) {
        self.tree = files::scan(&p);
        self.s.folder = Some(p);
        self.s.show_sidebar = true;
        self.side_tab = SideTab::Files;
    }

    fn add_recent(&mut self, p: PathBuf) {
        self.s.recent.retain(|x| x != &p);
        self.s.recent.insert(0, p);
        self.s.recent.truncate(10);
    }

    fn save_doc(&mut self, force_dialog: bool) -> bool {
        let path = match (&self.path, force_dialog) {
            (Some(p), false) => p.clone(),
            _ => {
                let name = self
                    .path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| format!("{}.md", self.suggested_name()));
                let mut dlg = rfd::FileDialog::new().add_filter("Markdown", &["md", "markdown"]).set_file_name(name);
                if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()).or(self.s.folder.as_deref()) {
                    dlg = dlg.set_directory(dir);
                }
                match dlg.save_file() {
                    Some(p) => p,
                    None => return false,
                }
            }
        };
        let text = self.current_text();
        match files::write(&path, &text, self.crlf) {
            Ok(()) => {
                self.saved_text = text;
                self.dirty = false;
                if self.path.as_ref() != Some(&path) {
                    self.path = Some(path.clone());
                    if let Some(f) = self.s.folder.clone() {
                        if path.starts_with(&f) {
                            self.tree = files::scan(&f);
                        }
                    }
                }
                self.add_recent(path);
                true
            }
            Err(e) => {
                self.notify(format!("Error while saving: {e}"));
                false
            }
        }
    }

    fn suggested_name(&self) -> String {
        let text = self.current_text();
        let d = Document::parse(&text);
        d.blocks
            .iter()
            .find(|b| Document::heading_level(&b.text).is_some())
            .map(|b| Document::heading_text(&b.text))
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().filter(|c| !"/\\:*?\"<>|".contains(*c)).take(60).collect())
            .unwrap_or_else(|| "Untitled".into())
    }

    fn export_html(&mut self) {
        let stem = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| self.suggested_name());
        let mut dlg = rfd::FileDialog::new().add_filter("HTML", &["html", "htm"]).set_file_name(format!("{stem}.html"));
        if let Some(dir) = self.path.as_ref().and_then(|p| p.parent()) {
            dlg = dlg.set_directory(dir);
        }
        if let Some(p) = dlg.save_file() {
            let html = files::to_html(&self.current_text(), &self.suggested_name(), &self.theme);
            match std::fs::write(&p, html) {
                Ok(()) => self.notify(format!("Exported {}", p.display())),
                Err(e) => self.notify(format!("Error while exporting: {e}")),
            }
        }
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn base_dir(&self) -> Option<PathBuf> {
        self.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf())).or_else(|| self.s.folder.clone())
    }

    // ───────────── block activation ─────────────

    fn editor_id(&self) -> Id {
        if self.source_mode {
            Id::unique(("source-editor", self.epoch))
        } else {
            Id::unique(("block-editor", self.epoch))
        }
    }

    /// Make block `target` the one being edited (commits the previous one).
    fn set_active(&mut self, target: Option<usize>, cur: Cur) {
        let mut target = target;
        if let Some(a) = self.active.take() {
            if a < self.doc.blocks.len() && target != Some(a) {
                if self.doc.blocks[a].text.trim().is_empty() && self.doc.blocks.len() > 1 {
                    let removed = self.doc.blocks.remove(a);
                    if a == self.doc.blocks.len() {
                        self.doc.blocks[a - 1].sep = removed.sep;
                    }
                    if let Some(t) = target {
                        if t > a {
                            target = Some(t - 1);
                        }
                    }
                } else {
                    let n = self.doc.resplit(a);
                    if let Some(t) = target {
                        if t > a && n > 1 {
                            target = Some(t + n - 1);
                        }
                    }
                }
            }
        }
        self.active = target.map(|t| t.min(self.doc.blocks.len().saturating_sub(1)));
        if let Some(t) = self.active {
            let len = self.doc.blocks[t].text.chars().count();
            self.pending_cursor = Some(match cur {
                Cur::Start => (0, 0),
                Cur::End => (len, len),
                Cur::At(a, b) => (a.min(len), b.min(len)),
            });
            self.just_activated = true;
        }
        self.epoch += 1;
    }

    fn insert_block_after(&mut self, i: Option<usize>, text: &str, cur: Cur) {
        let at = match i {
            Some(i) => i + 1,
            None => self.doc.blocks.len(),
        };
        let mut b = Block::new(text);
        if at >= self.doc.blocks.len() {
            if let Some(last) = self.doc.blocks.last_mut() {
                b.sep = std::mem::replace(&mut last.sep, "\n\n".into());
                if b.sep.is_empty() {
                    b.sep = "\n".into();
                }
            }
        }
        let at = at.min(self.doc.blocks.len());
        self.doc.blocks.insert(at, b);
        // keep `active` pointing at the same block
        if let Some(a) = self.active {
            if a >= at {
                self.active = Some(a + 1);
            }
        }
        self.set_active(Some(at), cur);
        self.changed(true);
    }

    // ───────────── editing commands ─────────────

    /// Apply `f` to the text of the active editor and its char selection.
    fn edit_active(&mut self, ctx: &Context, f: impl FnOnce(&mut String, std::ops::Range<usize>) -> std::ops::Range<usize>) -> bool {
        let id = self.editor_id();
        let text: &mut String = if self.source_mode {
            &mut self.source
        } else {
            match self.active {
                Some(a) if a < self.doc.blocks.len() => &mut self.doc.blocks[a].text,
                _ => return false,
            }
        };
        let len = text.chars().count();
        let sel = TextEditState::load(ctx, id)
            .and_then(|s| s.cursor.char_range())
            .map(|r| {
                let (a, b) = (r.primary.index.0.min(len), r.secondary.index.0.min(len));
                a.min(b)..a.max(b)
            })
            .unwrap_or(len..len);
        let new_sel = f(text, sel);
        self.pending_cursor = Some((new_sel.start, new_sel.end));
        self.changed(true);
        true
    }

    fn edit_paragraph(&mut self, ctx: &Context, f: impl FnOnce(&str) -> String) -> bool {
        let source = self.source_mode;
        self.edit_active(ctx, move |text, sel| {
            let range = if source { edit::para_bounds(text, char_to_byte(text, sel.start)) } else { 0..text.len() };
            let new = f(&text[range.clone()]);
            text.replace_range(range.clone(), &new);
            let end = byte_to_char(text, range.start + new.len());
            end..end
        })
    }

    fn insert_snippet(&mut self, ctx: &Context, snippet: &str, cursor_offset: usize) {
        if self.source_mode {
            let snippet = snippet.to_string();
            self.edit_active(ctx, move |text, sel| {
                let b = char_to_byte(text, sel.end);
                let para_end = edit::para_bounds(text, b).end;
                let ins = format!("\n\n{snippet}\n");
                text.insert_str(para_end, &ins);
                let c = byte_to_char(text, para_end) + 2 + cursor_offset;
                c..c
            });
        } else {
            let a = self.active;
            if let Some(i) = a {
                if self.doc.blocks[i].text.trim().is_empty() {
                    self.doc.blocks[i].text = snippet.to_string();
                    self.pending_cursor = Some((cursor_offset, cursor_offset));
                    self.epoch += 1;
                    self.changed(true);
                    return;
                }
            }
            self.insert_block_after(a, snippet, Cur::At(cursor_offset, cursor_offset));
        }
    }

    fn do_action(&mut self, ctx: &Context, a: Action) {
        match a {
            Action::New => self.guard(Pending::New),
            Action::Open => self.guard(Pending::Open(None)),
            Action::OpenPath(p) => {
                if self.path.as_ref() != Some(&p) {
                    self.guard(Pending::Open(Some(p)))
                }
            }
            Action::OpenFolder => {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.open_folder_path(p);
                }
            }
            Action::CloseFolder => {
                self.tree = None;
                self.s.folder = None;
                self.side_tab = SideTab::Outline;
            }
            Action::Save => {
                if self.save_doc(false) {
                    self.notify("Saved");
                }
            }
            Action::SaveAs => {
                if self.save_doc(true) {
                    self.notify("Saved");
                }
            }
            Action::ExportHtml => self.export_html(),
            Action::Quit => ctx.send_viewport_cmd(ViewportCommand::Close),
            Action::Undo | Action::Redo => {
                let cur = self.current_text();
                let (from, to) =
                    if a == Action::Undo { (&mut self.undo.undo, &mut self.undo.redo) } else { (&mut self.undo.redo, &mut self.undo.undo) };
                if let Some(t) = from.pop() {
                    to.push(cur);
                    let undo = std::mem::take(&mut self.undo.undo);
                    let redo = std::mem::take(&mut self.undo.redo);
                    let old_sel = TextEditState::load(ctx, self.editor_id()).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index.0);
                    self.load_text(t, self.path.clone(), true);
                    self.undo.undo = undo;
                    self.undo.redo = redo;
                    let len = if self.source_mode {
                        self.source.chars().count()
                    } else {
                        self.active.map(|i| self.doc.blocks[i].text.chars().count()).unwrap_or(0)
                    };
                    let c = old_sel.unwrap_or(len).min(len);
                    self.pending_cursor = Some((c, c));
                }
            }
            Action::Find => {
                self.find.open = true;
                self.find.replace = false;
                self.find.focus = true;
                self.prefill_find(ctx);
            }
            Action::Replace => {
                self.find.open = true;
                self.find.replace = true;
                self.find.focus = true;
                self.prefill_find(ctx);
            }
            Action::ToggleSource => self.toggle_source(),
            Action::ToggleSidebar => self.s.show_sidebar = !self.s.show_sidebar,
            Action::ToggleStatus => self.s.show_status = !self.s.show_status,
            Action::Focus => self.s.focus_mode = !self.s.focus_mode,
            Action::Typewriter => self.s.typewriter = !self.s.typewriter,
            Action::Autosave => self.s.autosave = !self.s.autosave,
            Action::Heading(l) => {
                if !self.edit_paragraph(ctx, |s| edit::set_heading(s, l)) {
                    self.notify("Place the cursor in a paragraph first");
                }
            }
            Action::Quote => {
                self.edit_paragraph(ctx, edit::toggle_quote);
            }
            Action::List(k) => {
                self.edit_paragraph(ctx, |s| edit::toggle_list(s, k));
            }
            Action::CodeFence => {
                let empty = self.active_is_empty();
                if empty || (!self.source_mode && self.active.is_none()) {
                    self.insert_snippet(ctx, "```\n\n```", 4);
                } else {
                    self.edit_paragraph(ctx, |s| edit::toggle_fence(s, "```"));
                }
            }
            Action::MathBlock => {
                let empty = self.active_is_empty();
                if empty || (!self.source_mode && self.active.is_none()) {
                    self.insert_snippet(ctx, "$$\n\n$$", 3);
                } else {
                    self.edit_paragraph(ctx, |s| edit::toggle_fence(s, "$$"));
                }
            }
            Action::Table => self.insert_snippet(ctx, "| Column 1 | Column 2 | Column 3 |\n| --- | --- | --- |\n|  |  |  |", 2),
            Action::Rule => self.insert_snippet(ctx, "---", 3),
            Action::Inline(m) => {
                self.edit_active(ctx, |t, s| edit::wrap_inline(t, s, m));
            }
            Action::Link => {
                self.edit_active(ctx, edit::make_link);
            }
            Action::Theme(k) => {
                self.s.theme = k;
                self.theme = Theme::get(k);
                self.theme.apply(ctx);
            }
            Action::FontBigger => self.set_font_size(self.s.font_size + 1.0),
            Action::FontSmaller => self.set_font_size(self.s.font_size - 1.0),
            Action::FontReset => self.set_font_size(16.0),
            Action::Wider => self.s.content_width = (self.s.content_width + 100.0).min(2000.0),
            Action::Narrower => self.s.content_width = (self.s.content_width - 100.0).max(500.0),
            Action::About => self.show_about = true,
            Action::Shortcuts => self.show_shortcuts = true,
        }
    }

    fn active_is_empty(&self) -> bool {
        if self.source_mode {
            false
        } else {
            self.active.and_then(|i| self.doc.blocks.get(i)).is_some_and(|b| b.text.trim().is_empty())
        }
    }

    fn set_font_size(&mut self, size: f32) {
        self.s.font_size = size.clamp(10.0, 32.0);
        self.sizes = Sizes::new(self.s.font_size);
    }

    fn toggle_source(&mut self) {
        let text = self.current_text();
        // Map the cursor between modes so the user keeps their place.
        if self.source_mode {
            self.source_mode = false;
            self.doc = Document::parse(&text);
            self.source.clear();
            self.active = None;
            self.epoch += 1;
        } else {
            let mut cursor = 0;
            if let Some(a) = self.active {
                let off = self.doc.block_offsets()[a];
                cursor = byte_to_char(&text, off);
            }
            self.set_active(None, Cur::End);
            self.source = self.doc.to_markdown();
            self.source_mode = true;
            self.epoch += 1;
            self.pending_cursor = Some((cursor, cursor));
            self.source_scroll_to = Some(cursor);
        }
    }

    /// Ask to save before an action that would discard changes.
    fn guard(&mut self, p: Pending) {
        if self.dirty {
            self.pending = Some(p);
        } else {
            self.perform(p);
        }
    }

    fn perform(&mut self, p: Pending) {
        match p {
            Pending::New => {
                self.source_mode = false;
                self.new_doc();
            }
            Pending::Open(path) => {
                let path = path.or_else(|| {
                    let mut d = rfd::FileDialog::new().add_filter("Markdown", files::MD_EXT).add_filter("All files", &["*"]);
                    if let Some(dir) = self.base_dir() {
                        d = d.set_directory(dir);
                    }
                    d.pick_file()
                });
                if let Some(p) = path {
                    self.open_path(&p);
                }
            }
            Pending::Quit => {
                self.allow_close = true;
            }
        }
    }

    fn prefill_find(&mut self, ctx: &Context) {
        let id = self.editor_id();
        let text = if self.source_mode { Some(self.source.as_str()) } else { self.active.map(|a| self.doc.blocks[a].text.as_str()) };
        if let (Some(text), Some(r)) = (text, TextEditState::load(ctx, id).and_then(|s| s.cursor.char_range())) {
            let (a, b) = (r.primary.index.0.min(r.secondary.index.0), r.primary.index.0.max(r.secondary.index.0));
            if a != b {
                let sel: String = text.chars().skip(a).take(b - a).collect();
                if !sel.contains('\n') {
                    self.find.query = sel;
                }
            }
        }
    }

    // ───────────── search ─────────────

    /// Global byte offset of the cursor.
    fn cursor_global(&self, ctx: &Context) -> usize {
        let c = TextEditState::load(ctx, self.editor_id())
            .and_then(|s| s.cursor.char_range())
            .map(|r| r.primary.index.0.max(r.secondary.index.0));
        if self.source_mode {
            char_to_byte(&self.source, c.unwrap_or(0))
        } else if let Some(a) = self.active {
            self.doc.block_offsets()[a] + char_to_byte(&self.doc.blocks[a].text, c.unwrap_or(0))
        } else {
            self.find.last.as_ref().map(|r| r.end).unwrap_or(0)
        }
    }

    fn find_step(&mut self, ctx: &Context, backwards: bool) {
        let text = self.current_text();
        let matches = edit::find_all(&text, &self.find.query, self.find.case);
        if matches.is_empty() {
            self.notify("Not found");
            return;
        }
        let pos = self.cursor_global(ctx);
        let m = if backwards {
            let start_from = self.find.last.as_ref().filter(|l| l.end == pos).map(|l| l.start).unwrap_or(pos);
            matches.iter().rev().find(|m| m.end <= start_from && m.start < start_from).or(matches.last()).cloned()
        } else {
            matches.iter().find(|m| m.start >= pos).or(matches.first()).cloned()
        };
        if let Some(m) = m {
            self.select_global(&text, m);
        }
    }

    fn select_global(&mut self, text: &str, m: std::ops::Range<usize>) {
        self.find.last = Some(m.clone());
        if self.source_mode {
            let a = byte_to_char(text, m.start);
            let b = byte_to_char(text, m.end);
            self.pending_cursor = Some((a, b));
            self.source_scroll_to = Some(a);
        } else {
            // commit first (lossless, offsets stay valid)
            self.set_active(None, Cur::End);
            let offs = self.doc.block_offsets();
            let k = offs.iter().rposition(|&o| o <= m.start).unwrap_or(0);
            let bt = &self.doc.blocks[k].text;
            let ls = (m.start - offs[k]).min(bt.len());
            let le = (m.end - offs[k]).min(bt.len());
            let (a, b) = (byte_to_char(bt, ls), byte_to_char(bt, le));
            self.set_active(Some(k), Cur::At(a, b));
            self.scroll_to = Some(k);
        }
    }

    fn replace_one(&mut self, ctx: &Context) {
        let text = self.current_text();
        if let Some(last) = self.find.last.clone() {
            let cur = text.get(last.clone()).unwrap_or("");
            let same = if self.find.case { cur == self.find.query } else { cur.to_lowercase() == self.find.query.to_lowercase() };
            if same && !cur.is_empty() {
                let mut t = text.clone();
                t.replace_range(last.clone(), &self.find.with);
                self.load_text(t.clone(), self.path.clone(), true);
                self.active = None;
                self.changed(true);
                let after = last.start + self.find.with.len();
                let matches = edit::find_all(&t, &self.find.query, self.find.case);
                if let Some(m) = matches.iter().find(|m| m.start >= after).or(matches.first()).cloned() {
                    self.select_global(&t, m);
                } else {
                    self.find.last = None;
                }
                return;
            }
        }
        self.find_step(ctx, false);
    }

    fn replace_all(&mut self) {
        let text = self.current_text();
        let matches = edit::find_all(&text, &self.find.query, self.find.case);
        if matches.is_empty() {
            self.notify("Not found");
            return;
        }
        let mut out = String::with_capacity(text.len());
        let mut pos = 0;
        for m in &matches {
            out.push_str(&text[pos..m.start]);
            out.push_str(&self.find.with);
            pos = m.end;
        }
        out.push_str(&text[pos..]);
        self.load_text(out, self.path.clone(), true);
        self.active = None;
        self.changed(true);
        self.notify(format!("Replaced {} matches", matches.len()));
    }

    // ───────────── UI pieces ─────────────

    fn menu_item(&self, ui: &mut egui::Ui, label: &str, a: Action, out: &mut Vec<Action>) {
        let mut b = egui::Button::new(label);
        if let Some(s) = shortcut_for(&a) {
            b = b.shortcut_text(ui.ctx().format_shortcut(&s));
        }
        if ui.add(b).clicked() {
            out.push(a);
            ui.close();
        }
    }

    fn menu_check(&self, ui: &mut egui::Ui, label: &str, on: bool, a: Action, out: &mut Vec<Action>) {
        let text = if on { format!("✔ {label}") } else { format!("    {label}") };
        self.menu_item(ui, &text, a, out);
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui, out: &mut Vec<Action>) {
        egui::Panel::top("menu").frame(egui::Frame::NONE.fill(self.theme.sidebar_bg).inner_margin(egui::Margin::symmetric(8, 3))).show(
            ui,
            |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        ui.set_min_width(240.0);
                        self.menu_item(ui, "New file", Action::New, out);
                        self.menu_item(ui, "Open…", Action::Open, out);
                        self.menu_item(ui, "Open folder…", Action::OpenFolder, out);
                        ui.menu_button("Open recent", |ui| {
                            if self.s.recent.is_empty() {
                                ui.label(RichText::new("(empty)").color(self.theme.muted));
                            }
                            for p in self.s.recent.clone() {
                                if ui.button(p.display().to_string()).clicked() {
                                    out.push(Action::OpenPath(p));
                                    ui.close();
                                }
                            }
                        });
                        if self.tree.is_some() {
                            self.menu_item(ui, "Close folder", Action::CloseFolder, out);
                        }
                        ui.separator();
                        self.menu_item(ui, "Save", Action::Save, out);
                        self.menu_item(ui, "Save as…", Action::SaveAs, out);
                        self.menu_check(ui, "Autosave", self.s.autosave, Action::Autosave, out);
                        ui.separator();
                        self.menu_item(ui, "Export HTML…", Action::ExportHtml, out);
                        ui.separator();
                        self.menu_item(ui, "Quit", Action::Quit, out);
                    });
                    ui.menu_button("Edit", |ui| {
                        ui.set_min_width(220.0);
                        self.menu_item(ui, "Undo", Action::Undo, out);
                        self.menu_item(ui, "Redo", Action::Redo, out);
                        ui.separator();
                        self.menu_item(ui, "Find…", Action::Find, out);
                        self.menu_item(ui, "Replace…", Action::Replace, out);
                    });
                    ui.menu_button("Paragraph", |ui| {
                        ui.set_min_width(240.0);
                        for l in 1..=6u8 {
                            self.menu_item(ui, &format!("Heading {l}"), Action::Heading(l), out);
                        }
                        self.menu_item(ui, "Normal paragraph", Action::Heading(0), out);
                        ui.separator();
                        self.menu_item(ui, "Table", Action::Table, out);
                        self.menu_item(ui, "Code block", Action::CodeFence, out);
                        self.menu_item(ui, "Math block", Action::MathBlock, out);
                        self.menu_item(ui, "Quote", Action::Quote, out);
                        ui.separator();
                        self.menu_item(ui, "Ordered list", Action::List(ListKind::Ordered), out);
                        self.menu_item(ui, "Bullet list", Action::List(ListKind::Bullet), out);
                        self.menu_item(ui, "Task list", Action::List(ListKind::Task), out);
                        ui.separator();
                        self.menu_item(ui, "Horizontal rule", Action::Rule, out);
                    });
                    ui.menu_button("Format", |ui| {
                        ui.set_min_width(220.0);
                        self.menu_item(ui, "Bold", Action::Inline("**"), out);
                        self.menu_item(ui, "Italic", Action::Inline("*"), out);
                        self.menu_item(ui, "Code", Action::Inline("`"), out);
                        self.menu_item(ui, "Strikethrough", Action::Inline("~~"), out);
                        self.menu_item(ui, "Inline math", Action::Inline("$"), out);
                        ui.separator();
                        self.menu_item(ui, "Link", Action::Link, out);
                    });
                    ui.menu_button("View", |ui| {
                        ui.set_min_width(260.0);
                        self.menu_check(ui, "Source mode", self.source_mode, Action::ToggleSource, out);
                        self.menu_check(ui, "Sidebar", self.s.show_sidebar, Action::ToggleSidebar, out);
                        self.menu_check(ui, "Status bar", self.s.show_status, Action::ToggleStatus, out);
                        ui.separator();
                        self.menu_check(ui, "Focus mode", self.s.focus_mode, Action::Focus, out);
                        self.menu_check(ui, "Typewriter mode", self.s.typewriter, Action::Typewriter, out);
                        ui.separator();
                        self.menu_item(ui, "Larger text", Action::FontBigger, out);
                        self.menu_item(ui, "Smaller text", Action::FontSmaller, out);
                        self.menu_item(ui, "Default text size", Action::FontReset, out);
                        self.menu_item(ui, "Wider content", Action::Wider, out);
                        self.menu_item(ui, "Narrower content", Action::Narrower, out);
                    });
                    ui.menu_button("Theme", |ui| {
                        for k in ThemeKind::ALL {
                            self.menu_check(ui, k.name(), self.s.theme == k, Action::Theme(k), out);
                        }
                    });
                    ui.menu_button("Help", |ui| {
                        self.menu_item(ui, "Keyboard shortcuts", Action::Shortcuts, out);
                        self.menu_item(ui, "About", Action::About, out);
                    });
                });
            },
        );
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, out: &mut Vec<Action>) {
        let text = self.current_text();
        let st = stats(&text);
        egui::Panel::bottom("status")
            .frame(egui::Frame::NONE.fill(self.theme.sidebar_bg).inner_margin(egui::Margin::symmetric(10, 4)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let small = |s: String| RichText::new(s).size(self.sizes.small).color(self.theme.muted);
                    if ui
                        .add(egui::Button::new(small((if self.s.show_sidebar { "◧" } else { "☰" }).into())).frame(false))
                        .on_hover_text("Sidebar")
                        .clicked()
                    {
                        out.push(Action::ToggleSidebar);
                    }
                    let name = self.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "Untitled".into());
                    ui.label(small(format!("{}{}", name, if self.dirty { "  •" } else { "" })));
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(egui::Button::new(small((if self.source_mode { "</>" } else { "👁" }).into())).frame(false))
                            .on_hover_text("Source mode (Ctrl+/)")
                            .clicked()
                        {
                            out.push(Action::ToggleSource);
                        }
                        let mins = (st.words as f32 / 200.0).ceil() as usize;
                        ui.label(small(format!(
                            "{} words   {} chars   {} lines   ~{} min read",
                            st.words,
                            st.chars,
                            st.lines,
                            mins.max(1)
                        )));
                        if self.s.focus_mode {
                            ui.label(small("Focus".into()));
                        }
                        if self.s.typewriter {
                            ui.label(small("Typewriter".into()));
                        }
                    });
                });
            });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, out: &mut Vec<Action>) {
        if !self.s.show_sidebar {
            return;
        }
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(240.0)
            .size_range(160.0..=500.0)
            .frame(egui::Frame::NONE.fill(self.theme.sidebar_bg).inner_margin(egui::Margin::same(10)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.side_tab, SideTab::Files, "Files");
                    ui.selectable_value(&mut self.side_tab, SideTab::Outline, "Outline");
                });
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.side_tab {
                    SideTab::Files => self.file_tree_ui(ui, out),
                    SideTab::Outline => self.outline_ui(ui),
                });
            });
    }

    fn file_tree_ui(&mut self, ui: &mut egui::Ui, out: &mut Vec<Action>) {
        match self.tree.clone() {
            None => {
                ui.label(RichText::new("No folder opened.").color(self.theme.muted));
                if ui.button("📂 Open folder…").clicked() {
                    out.push(Action::OpenFolder);
                }
                if !self.s.recent.is_empty() {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Recent").strong());
                    for p in self.s.recent.clone() {
                        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        let sel = self.path.as_ref() == Some(&p);
                        if ui.selectable_label(sel, format!("📄 {name}")).on_hover_text(p.display().to_string()).clicked() {
                            out.push(Action::OpenPath(p));
                        }
                    }
                }
            }
            Some(root) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&root.name).strong());
                    if ui.small_button("⟳").on_hover_text("Refresh").clicked() {
                        self.tree = files::scan(&root.path);
                    }
                });
                for c in &root.children {
                    self.tree_node(ui, c, out);
                }
                if root.children.is_empty() {
                    ui.label(RichText::new("(no Markdown files)").color(self.theme.muted));
                }
            }
        }
    }

    fn tree_node(&self, ui: &mut egui::Ui, n: &FileNode, out: &mut Vec<Action>) {
        if n.is_dir {
            egui::CollapsingHeader::new(format!("📁 {}", n.name))
                .id_salt(&n.path)
                .default_open(self.path.as_ref().is_some_and(|p| p.starts_with(&n.path)))
                .show(ui, |ui| {
                    for c in &n.children {
                        self.tree_node(ui, c, out);
                    }
                });
        } else {
            let sel = self.path.as_ref() == Some(&n.path);
            if ui.selectable_label(sel, format!("📄 {}", n.name)).clicked() {
                out.push(Action::OpenPath(n.path.clone()));
            }
        }
    }

    fn outline_ui(&mut self, ui: &mut egui::Ui) {
        let parsed;
        let (doc, offsets_src): (&Document, Option<&str>) = if self.source_mode {
            parsed = Document::parse(&self.source);
            (&parsed, Some(self.source.as_str()))
        } else {
            (&self.doc, None)
        };
        let offs = doc.block_offsets();
        let mut clicked = None;
        let mut any = false;
        for (i, b) in doc.blocks.iter().enumerate() {
            if let Some(level) = Document::heading_level(&b.text) {
                if edit::block_kind(&b.text) == Kind::Code {
                    continue;
                }
                any = true;
                let title = Document::heading_text(&b.text);
                let current = !self.source_mode && self.active == Some(i);
                ui.horizontal(|ui| {
                    ui.add_space((level as f32 - 1.0) * 14.0);
                    let rt = RichText::new(if title.is_empty() { "(empty)".to_string() } else { title });
                    let rt = if level <= 2 { rt.strong() } else { rt };
                    if ui.selectable_label(current, rt).clicked() {
                        clicked = Some((i, offs[i]));
                    }
                });
            }
        }
        if !any {
            ui.label(RichText::new("No headings yet.").color(self.theme.muted));
        }
        if let Some((i, off)) = clicked {
            if let Some(src) = offsets_src {
                let c = byte_to_char(src, off);
                self.pending_cursor = Some((c, c));
                self.source_scroll_to = Some(c);
            } else {
                self.scroll_to = Some(i);
            }
        }
    }

    fn find_bar(&mut self, ui: &mut egui::Ui) {
        if !self.find.open {
            return;
        }
        let mut step: Option<bool> = None;
        let mut rep_one = false;
        let mut rep_all = false;
        let text = self.current_text();
        let count = edit::find_all(&text, &self.find.query, self.find.case).len();
        egui::Panel::top("find").frame(egui::Frame::NONE.fill(self.theme.sidebar_bg).inner_margin(egui::Margin::symmetric(10, 6))).show(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("🔍");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.find.query).hint_text("Find…").desired_width(220.0));
                    if self.find.focus {
                        r.request_focus();
                        self.find.focus = false;
                    }
                    if r.changed() {
                        self.find.last = None;
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        step = Some(ui.input(|i| i.modifiers.shift));
                        r.request_focus();
                    }
                    ui.toggle_value(&mut self.find.case, "Aa").on_hover_text("Match case");
                    if ui.button("⬆").on_hover_text("Previous match (Shift+Enter)").clicked() {
                        step = Some(true);
                    }
                    if ui.button("⬇").on_hover_text("Next match (Enter)").clicked() {
                        step = Some(false);
                    }
                    let label = if self.find.query.is_empty() { String::new() } else { format!("{count} matches") };
                    ui.label(RichText::new(label).color(self.theme.muted));
                    if self.find.replace {
                        ui.separator();
                        ui.add(egui::TextEdit::singleline(&mut self.find.with).hint_text("Replace with…").desired_width(180.0));
                        if ui.button("Replace").clicked() {
                            rep_one = true;
                        }
                        if ui.button("Replace all").clicked() {
                            rep_all = true;
                        }
                    } else if ui.small_button("Replace…").clicked() {
                        self.find.replace = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("✕").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                            self.find.open = false;
                        }
                    });
                });
            },
        );
        if let Some(back) = step {
            self.find_step(ui.ctx(), back);
        }
        if rep_one {
            self.replace_one(ui.ctx());
        }
        if rep_all {
            self.replace_all();
        }
    }

    // ───────────── the editor ─────────────

    fn editor(&mut self, ui: &mut egui::Ui) {
        let frame = egui::Frame::NONE.fill(self.theme.bg).inner_margin(egui::Margin { left: 28, right: 28, top: 0, bottom: 0 });
        egui::CentralPanel::default().frame(frame).show(ui, |ui| {
            if self.source_mode {
                self.source_editor(ui);
            } else {
                self.block_editor(ui);
            }
        });
    }

    fn source_editor(&mut self, ui: &mut egui::Ui) {
        let id = self.editor_id();
        let theme = self.theme.clone();
        let sizes = self.sizes;
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
            let s = buf.as_str();
            let job = markdown_job(s, &theme, sizes, wrap, true);
            ui.ctx().fonts_mut(|f| f.layout_job(job))
        };
        egui::ScrollArea::vertical().id_salt("source-scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(24.0);
            let min_h = ui.available_height() - 40.0;
            let out = column(self.s.content_width, ui, |ui| {
                egui::TextEdit::multiline(&mut self.source)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .lock_focus(true)
                    .desired_width(f32::INFINITY)
                    .min_size(Vec2::new(0.0, min_h))
                    .layouter(&mut layouter)
                    .show(ui)
            });
            if let Some((a, b)) = self.pending_cursor.take() {
                let mut st = out.state.clone();
                st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
                st.store(ui.ctx(), id);
                out.response.request_focus();
            }
            if let Some(c) = self.source_scroll_to.take() {
                let r = out.galley.pos_from_cursor(CCursor::new(c)).translate(out.galley_pos.to_vec2());
                ui.scroll_to_rect(r.expand(60.0), Some(Align::Center));
            }
            if out.response.changed() {
                self.changed(false);
                if self.s.typewriter {
                    if let Some(cr) = out.cursor_range {
                        let r = out.galley.pos_from_cursor(cr.primary).translate(out.galley_pos.to_vec2());
                        ui.scroll_to_rect(r, Some(Align::Center));
                    }
                }
            }
            self.source_galley = Some((out.galley.clone(), out.galley_pos));
            ui.add_space(200.0);
        });
    }

    fn block_editor(&mut self, ui: &mut egui::Ui) {
        let n = self.doc.blocks.len();
        self.block_rects.resize(n, Rect::NOTHING);
        let gap = self.sizes.body * 0.75;
        egui::ScrollArea::vertical().id_salt("wysiwyg-scroll").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(28.0);
            let w = self.s.content_width;
            column(w, ui, |ui| {
                let mut i = 0;
                while i < self.doc.blocks.len() {
                    ui.push_id(("block", i), |ui| {
                        if self.active == Some(i) {
                            self.active_block(ui, i);
                        } else {
                            self.rendered_block(ui, i);
                        }
                    });
                    ui.add_space(gap);
                    i += 1;
                }
                // Clicking below the last block puts the cursor at the end.
                let h = (ui.available_height() - 20.0).max(160.0);
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click());
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(CursorIcon::Text);
                }
                let _ = rect;
                if resp.clicked() {
                    let last = self.doc.blocks.len() - 1;
                    if self.doc.blocks[last].text.trim().is_empty() {
                        self.set_active(Some(last), Cur::End);
                    } else {
                        self.insert_block_after(Some(last), "", Cur::Start);
                    }
                }
            });
        });
    }

    fn rendered_block(&mut self, ui: &mut egui::Ui, i: usize) {
        let prev = self.block_rects.get(i).copied().filter(|r| r.is_positive());
        let click = prev.map(|r| ui.interact(r, ui.scope_id().with("click"), Sense::click()));
        let text = self.doc.blocks[i].text.clone();
        let base_dir = self.base_dir();
        let dim = self.s.focus_mode && self.active.is_some();
        let resp = {
            let rc = Rc { theme: &self.theme, sizes: self.sizes, base_dir: base_dir.as_deref(), color: self.theme.text };
            ui.scope(|ui| {
                if dim {
                    ui.multiply_opacity(0.3);
                }
                if text.trim().is_empty() {
                    ui.allocate_exact_size(Vec2::new(ui.available_width(), self.sizes.body * 1.3), Sense::hover());
                    render::RenderOut::default()
                } else {
                    render::render_block(ui, &text, &rc)
                }
            })
        };
        let r = resp.response.rect;
        let full = Rect::from_min_max(egui::pos2(ui.min_rect().left(), r.top()), egui::pos2(ui.max_rect().right(), r.bottom()));
        if let Some(slot) = self.block_rects.get_mut(i) {
            *slot = full;
        }
        if self.scroll_to == Some(i) {
            ui.scroll_to_rect(full, Some(Align::TOP));
            self.scroll_to = None;
        }
        let out = resp.inner;
        if let Some(off) = out.toggle_task {
            render::toggle_task(&mut self.doc.blocks[i].text, off);
            self.changed(true);
            return;
        }
        if let Some(c) = click {
            if c.hovered() {
                let ctrl = ui.input(|i| i.modifiers.command);
                ui.ctx().set_cursor_icon(if ctrl && !out.links.is_empty() { CursorIcon::PointingHand } else { CursorIcon::Text });
                if !out.links.is_empty() && !ctrl {
                    c.clone().on_hover_text_at_pointer(format!("Ctrl+Click to open {}", out.links[0]));
                }
            }
            if c.clicked() {
                if ui.input(|i| i.modifiers.command) && !out.links.is_empty() {
                    let url = out.links[0].clone();
                    if url.starts_with("http") || url.starts_with("mailto:") {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    } else if let Some(dir) = base_dir {
                        let p = dir.join(url.split('#').next().unwrap_or(""));
                        if files::is_markdown(&p) && p.exists() {
                            self.guard(Pending::Open(Some(p)));
                        }
                    }
                } else {
                    // Approximate the click position: top half → start, bottom → end.
                    let pos = c.interact_pointer_pos().unwrap_or(full.center());
                    let cur = if pos.y < full.center().y && full.height() > self.sizes.body * 3.0 { Cur::Start } else { Cur::End };
                    self.set_active(Some(i), cur);
                }
            }
        }
    }

    fn active_block(&mut self, ui: &mut egui::Ui, i: usize) {
        let id = self.editor_id();
        let ctx = ui.ctx().clone();

        // Keyboard navigation between blocks (before the TextEdit sees the keys).
        if ctx.memory(|m| m.has_focus(id)) && self.pending_cursor.is_none() {
            if let Some(r) = TextEditState::load(&ctx, id).and_then(|s| s.cursor.char_range()) {
                let text = &self.doc.blocks[i].text;
                let c = r.primary.index.0;
                let collapsed = r.primary.index == r.secondary.index;
                let b = char_to_byte(text, c);
                let first_line = !text[..b].contains('\n');
                let last_line = !text[b..].contains('\n');
                let (up, down, left, right, bs) = ctx.input(|inp| {
                    let plain = !inp.modifiers.shift && !inp.modifiers.command && !inp.modifiers.alt;
                    (
                        plain && inp.key_pressed(Key::ArrowUp),
                        plain && inp.key_pressed(Key::ArrowDown),
                        plain && inp.key_pressed(Key::ArrowLeft),
                        plain && inp.key_pressed(Key::ArrowRight),
                        plain && inp.key_pressed(Key::Backspace),
                    )
                });
                let n = self.doc.blocks.len();
                let mut nav: Option<(usize, Cur)> = None;
                if collapsed && i > 0 && ((up && first_line) || (left && c == 0)) {
                    nav = Some((i - 1, Cur::End));
                } else if collapsed && i + 1 < n && ((down && last_line) || (right && b == text.len())) {
                    nav = Some((i + 1, Cur::Start));
                } else if collapsed && bs && c == 0 && i > 0 {
                    let prev = &self.doc.blocks[i - 1].text;
                    let prev_len = prev.chars().count();
                    let mergeable = matches!(edit::block_kind(prev), Kind::Paragraph | Kind::Heading)
                        && matches!(edit::block_kind(text), Kind::Paragraph);
                    if text.is_empty() || mergeable {
                        let moved = self.doc.blocks.remove(i);
                        self.doc.blocks[i - 1].text.push_str(&moved.text);
                        if i == self.doc.blocks.len() {
                            self.doc.blocks[i - 1].sep = moved.sep;
                        }
                        self.active = None;
                        self.set_active(Some(i - 1), Cur::At(prev_len, prev_len));
                        self.changed(true);
                        ctx.input_mut(|inp| inp.consume_key(Modifiers::NONE, Key::Backspace));
                        self.rendered_block(ui, i - 1);
                        return;
                    }
                }
                if let Some((t, cur)) = nav {
                    let key = if up {
                        Key::ArrowUp
                    } else if down {
                        Key::ArrowDown
                    } else if left {
                        Key::ArrowLeft
                    } else {
                        Key::ArrowRight
                    };
                    ctx.input_mut(|inp| inp.consume_key(Modifiers::NONE, key));
                    self.set_active(Some(t), cur);
                    // The block list may have changed; draw this one as rendered for now.
                    if i < self.doc.blocks.len() && self.active != Some(i) {
                        self.rendered_block(ui, i);
                    }
                    ctx.request_repaint();
                    return;
                }
            }
        }

        let theme = self.theme.clone();
        let sizes = self.sizes;
        let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
            let s = buf.as_str();
            let job = markdown_job(s, &theme, sizes, wrap, false);
            ui.ctx().fonts_mut(|f| f.layout_job(job))
        };
        let before = self.doc.blocks[i].text.clone();
        let only_block = self.doc.blocks.len() == 1;
        let mut text = std::mem::take(&mut self.doc.blocks[i].text);
        let frame = egui::Frame::NONE
            .fill(self.theme.active_bg)
            .corner_radius(3)
            .inner_margin(egui::Margin::symmetric(0, 2))
            .outer_margin(egui::Margin::symmetric(0, 0));
        let out = frame
            .show(ui, |ui| {
                egui::TextEdit::multiline(&mut text)
                    .id(id)
                    .frame(egui::Frame::NONE)
                    .lock_focus(true)
                    .desired_rows(1)
                    .desired_width(f32::INFINITY)
                    .hint_text(if only_block { "Start writing…" } else { "" })
                    .layouter(&mut layouter)
                    .show(ui)
            })
            .inner;
        self.doc.blocks[i].text = text;
        if let Some(slot) = self.block_rects.get_mut(i) {
            *slot = out.response.rect;
        }

        if let Some((a, b)) = self.pending_cursor.take() {
            let mut st = out.state.clone();
            st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
            st.store(&ctx, id);
            out.response.request_focus();
        }
        if self.just_activated {
            self.just_activated = false;
            ui.scroll_to_rect(out.response.rect, None);
        }
        if self.scroll_to == Some(i) {
            ui.scroll_to_rect(out.response.rect, Some(Align::TOP));
            self.scroll_to = None;
        }

        if out.response.changed() {
            let after = self.doc.blocks[i].text.clone();
            let cursor = out.cursor_range.map(|r| r.primary.index.0);
            let shift = ctx.input(|inp| inp.modifiers.shift);
            if let Some(c) = cursor {
                if after.len() == before.len() + 1 && c > 0 {
                    let b = char_to_byte(&after, c);
                    if after.as_bytes().get(b.wrapping_sub(1)) == Some(&b'\n') && after[..b - 1].to_string() + &after[b..] == before {
                        match edit::on_enter(&after, c, shift) {
                            EnterAction::Keep => {}
                            EnterAction::Rewrite { text, cursor } => {
                                self.doc.blocks[i].text = text;
                                self.pending_cursor = Some((cursor, cursor));
                            }
                            EnterAction::Split { before: head, after: tail } => {
                                self.doc.blocks[i].text = head;
                                self.insert_block_after(Some(i), &tail, Cur::Start);
                                return;
                            }
                        }
                    }
                }
            }
            self.changed(false);
            if self.s.typewriter {
                ui.scroll_to_rect(out.response.rect, Some(Align::Center));
            }
        }

        if out.response.lost_focus() && ctx.input(|inp| inp.key_pressed(Key::Escape)) {
            self.set_active(None, Cur::End);
        }
    }

    fn dialogs(&mut self, ctx: &Context) {
        if let Some(p) = self.pending.clone() {
            let mut close = false;
            egui::Window::new("Save changes?").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(
                ctx,
                |ui| {
                    let name = self
                        .path
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Untitled".into());
                    ui.label(format!("The document \"{name}\" has unsaved changes."));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() && self.save_doc(false) {
                            close = true;
                            self.perform(p.clone());
                        }
                        if ui.button("Don't save").clicked() {
                            close = true;
                            self.dirty = false;
                            self.perform(p.clone());
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                },
            );
            if close {
                self.pending = None;
                if self.allow_close {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
        if self.show_about {
            egui::Window::new("About")
                .collapsible(false)
                .resizable(false)
                .open(&mut self.show_about)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.heading("Rupora");
                    ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                    ui.label("A Typora-style Markdown editor written in Rust (egui + pulldown-cmark).");
                });
        }
        if self.show_shortcuts {
            egui::Window::new("Keyboard shortcuts").collapsible(false).open(&mut self.show_shortcuts).default_width(380.0).show(
                ctx,
                |ui| {
                    egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
                        egui::Grid::new("sc").striped(true).num_columns(2).show(ui, |ui| {
                            for (s, a) in shortcut_table() {
                                ui.monospace(ctx.format_shortcut(&s.sc));
                                ui.label(describe(&a));
                                ui.end_row();
                            }
                            for (k, d) in [
                                ("Enter", "New paragraph / new list item"),
                                ("Shift+Enter", "Line break within a paragraph"),
                                ("↑ ↓ ← →", "Move between blocks"),
                                ("Esc", "Stop editing the block"),
                                ("Ctrl+Click", "Open link"),
                            ] {
                                ui.monospace(k);
                                ui.label(d);
                                ui.end_row();
                            }
                        });
                    });
                },
            );
        }
        if let Some((msg, t)) = &self.toast {
            if t.elapsed() < Duration::from_millis(2500) {
                egui::Area::new(Id::unique("toast")).anchor(Align2::RIGHT_BOTTOM, Vec2::new(-20.0, -40.0)).show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(msg));
                });
                ctx.request_repaint_after(Duration::from_millis(200));
            } else {
                self.toast = None;
            }
        }
    }

    fn update_title(&mut self, ctx: &Context) {
        let name =
            self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into());
        let title = format!("{}{} — Rupora", if self.dirty { "• " } else { "" }, name);
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        let mut actions: Vec<Action> = Vec::new();

        // Global shortcuts first, so the text editors never see them.
        ctx.input_mut(|i| {
            for (s, a) in shortcut_table() {
                if i.consume_shortcut(&s.sc) {
                    actions.push(a);
                }
            }
        });
        if self.find.open && ctx.input(|i| i.key_pressed(Key::F3)) {
            let back = ctx.input(|i| i.modifiers.shift);
            self.find_step(ctx, back);
        }

        // Drag & drop
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(p) = dropped.into_iter().next() {
            if p.is_dir() {
                self.open_folder_path(p);
            } else {
                actions.push(Action::OpenPath(p));
            }
        }

        // Closing the window with unsaved changes
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty && !self.allow_close {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }

        self.menu_bar(ui, &mut actions);
        if self.s.show_status {
            self.status_bar(ui, &mut actions);
        }
        self.find_bar(ui);
        self.sidebar(ui, &mut actions);
        self.editor(ui);
        self.dialogs(ctx);

        for a in actions {
            self.do_action(ctx, a);
        }

        if self.s.autosave && self.dirty && self.path.is_some() {
            if self.last_change.elapsed() > Duration::from_secs(2) {
                self.save_doc(false);
            } else {
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
        self.update_title(ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, SETTINGS_KEY, &self.s);
    }

    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(30)
    }
}
