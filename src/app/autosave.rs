use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use adw::prelude::*;
use gtk4::glib::{self, ControlFlow};
use gtk4::{self as gtk, prelude::*};
use libadwaita as adw;
use serde::{Deserialize, Serialize};
use serde_json;

use super::window::AppState;

pub(super) const CUSTOM_AUTOSAVE_SENTINEL: u64 = u64::MAX;
const AUTOSAVE_IDLE_GRACE_SECS: u64 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AutosaveMetadata {
    pub(super) original_path: Option<String>,
    pub(super) timestamp: u64,
}

impl AppState {
    pub(super) fn set_autosave_interval(self: &Rc<Self>, secs: u64) {
        if secs == CUSTOM_AUTOSAVE_SENTINEL {
            self.prompt_custom_autosave();
            return;
        }
        {
            let mut settings = self.settings.borrow_mut();
            if settings.autosave_interval_secs == secs {
                // no change
            } else {
                settings.autosave_interval_secs = secs;
                if let Err(err) = settings.save(&self.paths) {
                    log::warn!("Failed to save settings: {err:?}");
                }
            }
        }
        self.restart_autosave();
        self.sync_preferences_ui();
        let desc = self.autosave_description(secs);
        self.show_toast(&format!("Autosave {desc}"));
    }

    pub(super) fn set_autosave_idle_only(self: &Rc<Self>, active: bool) {
        {
            let mut settings = self.settings.borrow_mut();
            if settings.autosave_idle_only == active {
                return;
            }
            settings.autosave_idle_only = active;
            if let Err(err) = settings.save(&self.paths) {
                log::warn!("Failed to save settings: {err:?}");
            }
        }
        self.sync_preferences_ui();
        if active {
            self.show_toast("Autosave waits for idle typing");
        } else {
            self.show_toast("Autosave runs continuously");
        }
    }

    pub(super) fn restart_autosave(self: &Rc<Self>) {
        if let Some(source) = self.autosave_source.borrow_mut().take() {
            // Ignore errors if source was already removed
            let _ = source.remove();
        }
        let interval = self.settings.borrow().autosave_interval_secs;
        if interval == 0 {
            // Autosave disabled
            return;
        }
        // Label removed from status bar, but logic continues
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_seconds_local(interval as u32, move || {
            if let Some(state) = weak.upgrade() {
                state.run_autosave();
                ControlFlow::Continue
            } else {
                ControlFlow::Break
            }
        });
        self.autosave_source.replace(Some(id));
    }

    pub(super) fn run_autosave(&self) {
        if !self.buffer.is_modified() {
            return;
        }
        if self.settings.borrow().autosave_idle_only {
            if let Some(last) = *self.last_edit.borrow() {
                if last.elapsed() < Duration::from_secs(AUTOSAVE_IDLE_GRACE_SECS) {
                    // Waiting for idle
                    return;
                }
            }
        }
        match self.write_autosave_file() {
            Ok(_timestamp) => {
                // Autosave success
            }
            Err(err) => {
                log::warn!("Autosave error: {err:?}");
            }
        }
    }

    fn write_autosave_file(&self) -> anyhow::Result<String> {
        let data = self.document.current_text();
        let swap_path = self.autosave_path();
        let temp = swap_path.with_extension("tmp");
        fs::write(&temp, &data)?;
        fs::rename(&temp, &swap_path)?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        let metadata = AutosaveMetadata {
            original_path: self
                .file_path
                .borrow()
                .as_ref()
                .map(|p| p.display().to_string()),
            timestamp: ts,
        };
        let meta_path = self.autosave_metadata_path(&swap_path);
        fs::write(&meta_path, serde_json::to_string(&metadata)?)?;
        Ok(format!("{}s", ts))
    }

    pub(super) fn autosave_path(&self) -> PathBuf {
        let name = self
            .file_path
            .borrow()
            .as_ref()
            .and_then(|p| p.file_name().and_then(|o| o.to_str()))
            .map(|s| s.to_string())
            .unwrap_or_else(|| "untitled".to_string());
        let sanitized = name.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
        self.paths
            .autosave_dir
            .join(format!(".{sanitized}-{}.swap", self.session_token))
    }

    pub(super) fn autosave_metadata_path(&self, swap_path: &Path) -> PathBuf {
        swap_path.with_extension("meta")
    }

    pub(super) fn remove_autosave_artifacts(&self) {
        let swap = self.autosave_path();
        if swap.exists() {
            let _ = fs::remove_file(&swap);
        }
        let meta = self.autosave_metadata_path(&swap);
        if meta.exists() {
            let _ = fs::remove_file(&meta);
        }
    }

    pub(super) fn autosave_description(&self, secs: u64) -> String {
        if let Some((_, label)) = self.autosave_options.iter().find(|(v, _)| *v == secs) {
            label.to_string()
        } else if secs == 0 {
            "Off".to_string()
        } else {
            format!("every {}s", secs)
        }
    }

    pub(super) fn sync_preferences_ui(&self) {
        if let Some(idx) = self.find_interval_index(self.settings.borrow().autosave_interval_secs) {
            self.preferences.autosave_combo.set_selected(idx as u32);
        }
        self.preferences
            .autosave_idle_switch
            .set_active(self.settings.borrow().autosave_idle_only);
    }

    pub(super) fn find_interval_index(&self, secs: u64) -> Option<usize> {
        self.autosave_options
            .iter()
            .position(|(value, _)| *value == secs)
            .or_else(|| {
                self.autosave_options
                    .iter()
                    .position(|(value, _)| *value == CUSTOM_AUTOSAVE_SENTINEL)
            })
    }

    pub(super) fn prompt_custom_autosave(self: &Rc<Self>) {
        let dialog = gtk::Dialog::builder()
            .title("Custom Autosave Interval")
            .transient_for(&self.window())
            .modal(true)
            .build();
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Save", gtk::ResponseType::Accept);
        dialog.set_default_response(gtk::ResponseType::Accept);
        let entry = gtk::Entry::builder()
            .placeholder_text("Seconds")
            .input_purpose(gtk::InputPurpose::Digits)
            .activates_default(true)
            .build();
        entry.set_margin_top(12);
        entry.set_margin_bottom(12);
        entry.set_margin_start(12);
        entry.set_margin_end(12);
        entry.set_text(&self.settings.borrow().autosave_interval_secs.to_string());
        dialog.content_area().append(&entry);
        entry.grab_focus();
        let weak = Rc::downgrade(self);
        let entry_clone = entry.clone();
        dialog.connect_response(move |dialog, response| {
            if let Some(state) = weak.upgrade() {
                if response == gtk::ResponseType::Accept {
                    let text = entry_clone.text();
                    match text.trim().parse::<u64>() {
                        Ok(value) if value > 0 => state.set_autosave_interval(value),
                        _ => state
                            .status_label
                            .set_text("Enter a positive number of seconds"),
                    }
                }
                state.sync_preferences_ui();
            }
            dialog.close();
        });
        dialog.show();
    }
}
