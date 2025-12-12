use std::path::Path;

use gtk4::{self as gtk, prelude::*};

use super::window::AppState;

impl AppState {
    pub(super) fn record_recent_file(&self, path: &Path) {
        let mut entries = self.recent_entries.borrow_mut();
        entries.retain(|p| p != path);
        entries.insert(0, path.to_path_buf());
        if entries.len() > 10 {
            entries.truncate(10);
        }
        {
            let mut settings = self.settings.borrow_mut();
            settings.recent_files = entries.iter().map(|p| p.display().to_string()).collect();
            if let Err(err) = settings.save(&self.paths) {
                log::warn!("Failed to save settings: {err:?}");
            }
        }
        drop(entries);
        self.refresh_recent_menu();
    }

    pub(super) fn refresh_recent_menu(&self) {
        while let Some(child) = self.recent_list.first_child() {
            self.recent_list.remove(&child);
        }
        let entries = self.recent_entries.borrow();
        if entries.is_empty() {
            // No recent files
            let label = gtk::Label::new(Some("No recent files"));
            label.set_margin_top(8);
            label.set_margin_bottom(8);
            label.set_margin_start(12);
            label.set_margin_end(12);
            let row = gtk::ListBoxRow::builder()
                .activatable(false)
                .selectable(false)
                .build();
            row.set_child(Some(&label));
            self.recent_list.append(&row);
            return;
        }
        // List logic continues
        for path in entries.iter() {
            let display = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let subtitle = path.display().to_string();
            let vbox = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(0)
                .margin_top(6)
                .margin_bottom(6)
                .margin_start(12)
                .margin_end(12)
                .build();
            let title_label = gtk::Label::new(Some(&display));
            title_label.set_xalign(0.0);
            let path_label = gtk::Label::new(Some(&subtitle));
            path_label.set_xalign(0.0);
            path_label.add_css_class("dim-label");
            vbox.append(&title_label);
            vbox.append(&path_label);
            let row = gtk::ListBoxRow::builder()
                .activatable(true)
                .selectable(false)
                .build();
            row.set_child(Some(&vbox));
            self.recent_list.append(&row);
        }
    }
}
