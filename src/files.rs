//! File-system helpers: folder tree for the sidebar, reading/writing files and
//! HTML export.

use crate::document::md_options;
use crate::theme::Theme;
use std::path::{Path, PathBuf};

pub const MD_EXT: &[&str] = &["md", "markdown", "mdown", "mkd", "mkdn", "txt"];

pub fn is_markdown(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| MD_EXT.contains(&e.to_ascii_lowercase().as_str()))
}

#[derive(Debug, Clone)]
pub struct FileNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
}

pub fn scan(path: &Path) -> Option<FileNode> {
    scan_inner(path, 0)
}

fn scan_inner(path: &Path, depth: usize) -> Option<FileNode> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
    if path.is_dir() {
        if depth > 8 {
            return None;
        }
        let mut children: Vec<FileNode> = std::fs::read_dir(path)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let n = e.file_name();
                let n = n.to_string_lossy();
                !n.starts_with('.') && n != "node_modules" && n != "target"
            })
            .filter_map(|e| scan_inner(&e.path(), depth + 1))
            .collect();
        children.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        if depth > 0 && children.is_empty() {
            return None;
        }
        Some(FileNode { path: path.to_path_buf(), name, is_dir: true, children })
    } else if is_markdown(path) {
        Some(FileNode { path: path.to_path_buf(), name, is_dir: false, children: Vec::new() })
    } else {
        None
    }
}

pub struct Loaded {
    pub text: String,
    pub crlf: bool,
}

pub fn read(path: &Path) -> std::io::Result<Loaded> {
    let bytes = std::fs::read(path)?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if text.starts_with('\u{feff}') {
        text.remove(0);
    }
    let crlf = text.contains("\r\n");
    if crlf {
        text = text.replace("\r\n", "\n");
    }
    Ok(Loaded { text, crlf })
}

pub fn write(path: &Path, text: &str, crlf: bool) -> std::io::Result<()> {
    let data = if crlf { text.replace('\n', "\r\n") } else { text.to_string() };
    // Write atomically: temp file + rename.
    let tmp = path.with_extension(format!("{}.rupora-tmp", path.extension().and_then(|e| e.to_str()).unwrap_or("md")));
    std::fs::write(&tmp, data.as_bytes())?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) => {
            let _ = std::fs::remove_file(&tmp);
            std::fs::write(path, data.as_bytes())
        }
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn to_html(markdown: &str, title: &str, theme: &Theme) -> String {
    let parser = pulldown_cmark::Parser::new_ext(markdown, md_options());
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, parser);
    let has_math = body.contains("class=\"math");
    let katex = if has_math {
        r#"<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.css">
<script defer src="https://cdn.jsdelivr.net/npm/katex@0.16.11/dist/katex.min.js"
  onload="document.querySelectorAll('.math').forEach(e=>{try{katex.render(e.textContent,e,{displayMode:e.classList.contains('math-display'),throwOnError:false})}catch(_){}})"></script>
"#
    } else {
        ""
    };
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<style>\n{}</style>\n{}</head>\n<body>\n{}</body>\n</html>\n",
        escape_html(title),
        theme.export_css(),
        katex,
        body
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeKind;

    #[test]
    fn html_export() {
        let h = to_html("# Hi <3 & you\n\n- [x] done\n\n$a$", "T<", &Theme::get(ThemeKind::Github));
        assert!(h.contains("<h1>Hi &lt;3 &amp; you</h1>"), "{h}");
        assert!(h.contains("<title>T&lt;</title>"), "{h}");
        assert!(h.contains("checkbox"), "{h}");
        assert!(h.contains("katex"), "{h}");
    }

    #[test]
    fn rw_roundtrip() {
        let dir = std::env::temp_dir().join(format!("rupora-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.md");
        write(&p, "x\ny\n", true).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"x\r\ny\r\n");
        let l = read(&p).unwrap();
        assert!(l.crlf);
        assert_eq!(l.text, "x\ny\n");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/b.markdown"), "b").unwrap();
        std::fs::write(dir.join("ignore.png"), "b").unwrap();
        let t = scan(&dir).unwrap();
        assert_eq!(t.children.len(), 2);
        assert!(t.children[0].is_dir);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
