use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use gtk4::{self as gtk, prelude::*};
use serde_json;

use super::autosave::AutosaveMetadata;
use super::window::AppState;

#[derive(Debug, Clone)]
pub(super) struct RecoveryEntry {
    pub(super) swap_path: PathBuf,
    pub(super) meta_path: PathBuf,
    pub(super) metadata: AutosaveMetadata,
}

impl AppState {
    pub(super) fn check_recovery_snapshots(self: &Rc<Self>) {
        let entries = match self.collect_recovery_entries() {
            Ok(entries) => entries,
            Err(err) => {
                log::warn!("Failed to inspect autosave snapshots: {err:?}");
                return;
            }
        };
        if entries.is_empty() {
            return;
        }
        let queue = Rc::new(RefCell::new(entries));
        self.present_next_recovery(queue);
    }

    fn collect_recovery_entries(&self) -> anyhow::Result<Vec<RecoveryEntry>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(&self.paths.autosave_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("swap") {
                continue;
            }
            if self.swap_belongs_to_current_session(&path) {
                continue;
            }
            let meta_path = self.autosave_metadata_path(&path);
            let metadata = fs::read_to_string(&meta_path)
                .ok()
                .and_then(|raw| serde_json::from_str::<AutosaveMetadata>(&raw).ok())
                .unwrap_or(AutosaveMetadata {
                    original_path: None,
                    timestamp: 0,
                });
            entries.push(RecoveryEntry {
                swap_path: path,
                meta_path,
                metadata,
            });
        }
        entries.sort_by_key(|entry| entry.metadata.timestamp);
        entries.reverse();
        Ok(entries)
    }

    fn swap_belongs_to_current_session(&self, path: &Path) -> bool {
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if let Some(pos) = stem.rfind('-') {
                let session = &stem[pos + 1..];
                return session == self.session_token;
            }
        }
        false
    }

    fn present_next_recovery(self: &Rc<Self>, entries: Rc<RefCell<Vec<RecoveryEntry>>>) {
        let entry = match entries.borrow_mut().pop() {
            Some(e) => e,
            None => return,
        };
        let description = entry.metadata.description();
        let dialog = gtk::MessageDialog::builder()
            .transient_for(&self.window)
            .modal(true)
            .text("Recovered autosave found")
            .secondary_text(&description)
            .build();
        dialog.add_button("Discard", gtk::ResponseType::Reject);
        dialog.add_button("Restore", gtk::ResponseType::Accept);
        let weak = Rc::downgrade(self);
        dialog.connect_response(move |dialog, response| {
            if let Some(state) = weak.upgrade() {
                match response {
                    gtk::ResponseType::Accept => state.restore_recovery_entry(&entry),
                    _ => state.discard_recovery_entry(&entry),
                }
                state.present_next_recovery(entries.clone());
            }
            dialog.close();
        });
        dialog.show();
    }

    fn restore_recovery_entry(&self, entry: &RecoveryEntry) {
        match fs::read_to_string(&entry.swap_path) {
            Ok(contents) => {
                self.document.buffer().set_text(&contents);
                self.buffer.set_modified(true);
                if let Some(path) = entry.metadata.original_path.as_ref().map(PathBuf::from) {
                    self.file_path.replace(Some(path));
                } else {
                    self.file_path.replace(None);
                }
                self.update_title();
                self.last_edit.replace(Some(Instant::now()));
                self.show_toast("Recovered autosave applied");
            }
            Err(err) => self.present_error("Failed to restore", &err.to_string()),
        }
        self.discard_recovery_entry(entry);
    }

    fn discard_recovery_entry(&self, entry: &RecoveryEntry) {
        if entry.swap_path.exists() {
            let _ = fs::remove_file(&entry.swap_path);
        }
        if entry.meta_path.exists() {
            let _ = fs::remove_file(&entry.meta_path);
        }
    }
}

impl AutosaveMetadata {
    pub(super) fn description(&self) -> String {
        let location = self.original_path.as_deref().unwrap_or("Untitled document");
        if self.timestamp == 0 {
            format!("Snapshot for {location}")
        } else {
            let dt = UNIX_EPOCH + Duration::from_secs(self.timestamp);
            match dt.duration_since(UNIX_EPOCH) {
                Ok(_) => format!("Snapshot for {location} ({}s since epoch)", self.timestamp),
                Err(_) => format!("Snapshot for {location}"),
            }
        }
    }
}
