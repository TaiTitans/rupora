# Welcome to Rupora

Rupora is a *Typora*-style **Markdown** editor written entirely in Rust. The paragraph your cursor is in shows the raw Markdown source, while every other paragraph is displayed as a preview.

## Basic formatting

You can use **bold**, *italic*, ~~strikethrough~~, `inline code`, [links](https://www.rust-lang.org) and math like $E = mc^2$.

> Quote: "Programs must be written for people to read, and only incidentally for machines to execute."
> — Harold Abelson

## Lists

- Press **Enter** at the end of an item to create a new one
- Press **Enter** on an empty item to leave the list
  - Nested lists are supported too

1. Open a file with `Ctrl+O`
2. Save with `Ctrl+S`
3. Toggle source mode with `Ctrl+/`

- [x] Click the checkbox to mark a task
- [ ] Try focus mode (`F8`) and typewriter mode (`F9`)

## Code blocks

```rust
fn main() {
    let name = "Rupora";
    println!("Hello from {name}!"); // syntax highlighting
}
```

## Tables

| Shortcut | Action | Note |
|:---------|:------:|-----:|
| `Ctrl+B` | Bold | `**…**` |
| `Ctrl+I` | Italic | `*…*` |
| `Ctrl+1…6` | Heading | `#…` |
| `Ctrl+F` | Find | `Ctrl+H` to replace |

---

Happy writing! ✍️
