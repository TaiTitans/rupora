//! Rupora — a Typora-style WYSIWYG Markdown editor written in Rust.

mod app;
mod document;
mod edit;
mod files;
mod fonts;
mod highlight;
mod render;
mod theme;

fn main() -> eframe::Result<()> {
    let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rupora")
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([480.0, 320.0])
            .with_drag_and_drop(true),
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native("Rupora", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, open)))))
}
