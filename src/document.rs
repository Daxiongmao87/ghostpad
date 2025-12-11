use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use gtk4::prelude::*;
use sourceview5::{Buffer, View};

use std::rc::Rc;

pub struct Document {
    buffer: Buffer,
    view: View,
}

impl Document {
    pub fn new() -> Rc<Self> {
        let buffer = Buffer::builder().highlight_syntax(true).build();
        let view = View::builder()
            .buffer(&buffer)
            .monospace(true)
            .show_line_numbers(true)
            .wrap_mode(gtk4::WrapMode::WordChar)
            .build();
        view.set_vexpand(true);
        view.set_hexpand(true);
        Rc::new(Self { buffer, view })
    }

    pub fn view(&self) -> View {
        self.view.clone()
    }

    pub fn buffer(&self) -> Buffer {
        self.buffer.clone()
    }

    pub fn clear(&self) {
        self.buffer.set_text("");
        self.buffer.set_modified(false);
    }

    pub fn load_from_path(&self, path: &Path) -> Result<()> {
        let data = fs::read_to_string(path)
            .with_context(|| format!("Failed to open {}", path.display()))?;
        self.buffer.set_text(&data);
        self.buffer.set_modified(false);
        Ok(())
    }

    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        let text = self.current_text();
        fs::write(path, text).with_context(|| format!("Failed to save {}", path.display()))?;
        self.buffer.set_modified(false);
        Ok(())
    }

    pub fn current_text(&self) -> String {
        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        self.buffer.text(&start, &end, true).to_string()
    }
}

pub fn derive_display_name(path: &Option<PathBuf>) -> String {
    match path {
        Some(p) => p
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| p.display().to_string()),
        None => "Untitled".to_string(),
    }
}
