use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use gtk4::gdk::RGBA;
use gtk4::pango::Style;
use gtk4::prelude::*;
use sourceview5::{Buffer, View};

use std::cell::RefCell;
use std::rc::Rc;

pub struct Document {
    buffer: Buffer,
    view: View,
    ghost_tag: gtk4::TextTag,
    ghost_range: RefCell<Option<(gtk4::TextMark, gtk4::TextMark)>>,
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

        let tag_table = buffer.tag_table();
        let ghost_tag = gtk4::TextTag::builder()
            .name("llm-ghost")
            .style(Style::Italic)
            .build();
        ghost_tag.set_property("foreground-rgba", &RGBA::new(0.53, 0.53, 0.53, 1.0));
        tag_table.add(&ghost_tag);

        Rc::new(Self {
            buffer,
            view,
            ghost_tag,
            ghost_range: RefCell::new(None),
        })
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

    pub fn insert_ghost_text(&self, text: &str) {
        self.dismiss_ghost_text();
        if text.is_empty() {
            return;
        }

        // Get cursor position using the insert mark (always valid)
        let insert_mark = self.buffer.get_insert();
        let mut insert_iter = self.buffer.iter_at_mark(&insert_mark);

        // Create start mark before insertion (left-gravity so it stays at start of inserted text)
        // We use the iterator we just got, which is valid.
        let start_mark = self.buffer.create_mark(None, &insert_iter, true);

        // Insert text with tags
        // Note: insert_with_tags invalidates iterators, but updates the one passed to it.
        // However, to be absolutely safe against any binding quirks or signals,
        // we will re-acquire the iterator from the insert mark (which moves with the insertion).
        self.buffer
            .insert_with_tags(&mut insert_iter, text, &[&self.ghost_tag]);

        // Re-acquire iter from the insert mark, which is now at the end of the insertion
        // because the 'insert' mark has right gravity (moves with text).
        let end_iter = self.buffer.iter_at_mark(&insert_mark);
        let end_mark = self.buffer.create_mark(None, &end_iter, true);

        // Restore cursor to start of ghost text (so user is "before" the suggestion)
        // We use the start_mark we just created.
        let start_iter = self.buffer.iter_at_mark(&start_mark);
        self.buffer.place_cursor(&start_iter);

        self.ghost_range.replace(Some((start_mark, end_mark)));
    }

    pub fn ghost_is_active(&self) -> bool {
        self.ghost_range.borrow().is_some()
    }

    pub fn accept_ghost_text(&self) -> bool {
        if let Some((start_mark, end_mark)) = self.take_ghost_marks() {
            // Validate marks are not deleted
            if start_mark.is_deleted() || end_mark.is_deleted() {
                log::warn!("Ghost text marks already deleted in accept_ghost_text");
                return false;
            }

            let mut start = self.buffer.iter_at_mark(&start_mark);
            let mut end = self.buffer.iter_at_mark(&end_mark);
            self.buffer
                .remove_tag(&self.ghost_tag, &mut start, &mut end);
            // Move cursor to end of accepted text
            self.buffer.place_cursor(&end);
            self.buffer.delete_mark(&start_mark);
            self.buffer.delete_mark(&end_mark);
            return true;
        }
        false
    }

    pub fn dismiss_ghost_text(&self) {
        if let Some((start_mark, end_mark)) = self.take_ghost_marks() {
            // Validate marks are not deleted
            if start_mark.is_deleted() || end_mark.is_deleted() {
                log::warn!("Ghost text marks already deleted in dismiss_ghost_text");
                return;
            }

            let mut start = self.buffer.iter_at_mark(&start_mark);
            let mut end = self.buffer.iter_at_mark(&end_mark);
            self.buffer.delete(&mut start, &mut end);
            self.buffer.delete_mark(&start_mark);
            self.buffer.delete_mark(&end_mark);
        }
    }

    fn take_ghost_marks(&self) -> Option<(gtk4::TextMark, gtk4::TextMark)> {
        self.ghost_range.borrow_mut().take()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ghost_text_insertion() {
        // Initialize GTK (if possible in this env)
        if gtk4::init().is_err() {
            // If no display, skip test but warn
            eprintln!("Skipping GTK test due to missing display");
            return;
        }

        let doc = Document::new();
        doc.buffer.set_text("Hello World");
        
        // Move cursor to "Hello"
        let mut iter = doc.buffer.iter_at_offset(5);
        doc.buffer.place_cursor(&iter);

        // Insert ghost text
        doc.insert_ghost_text(" Beautiful");

        // Verify content
        // Ghost text is in the buffer but tagged.
        let text = doc.current_text();
        assert_eq!(text, "Hello Beautiful World");

        // Verify marks
        assert!(doc.ghost_is_active());
        
        // Dismiss
        doc.dismiss_ghost_text();
        let text_after = doc.current_text();
        assert_eq!(text_after, "Hello World");
    }
}
