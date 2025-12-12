use sourceview5::prelude::*;

use super::window::AppState;

impl AppState {
    pub(super) fn update_search_pattern(&self) {
        let pattern = self.search_entry.text();
        if pattern.is_empty() {
            self.search_settings.set_search_text(None::<&str>);
        } else {
            self.search_settings.set_search_text(Some(pattern.as_str()));
        }
        self.search_context.set_highlight(!pattern.is_empty());
        self.update_search_feedback();
    }

    pub(super) fn update_search_feedback(&self) {
        let pattern = self.search_entry.text();
        if pattern.is_empty() {
            self.match_label.set_text("0 matches");
            return;
        }
        if let Some(err) = self.search_context.regex_error() {
            self.match_label
                .set_text(&format!("Regex error: {}", err.message()));
            self.status_label
                .set_text(&format!("Regex error: {}", err.message()));
        } else {
            let count = self.search_context.occurrences_count();
            self.match_label
                .set_text(&format!("{} matches", count.max(0)));
        }
    }

    pub(super) fn find_next_match(&self, forward: bool) {
        if self.search_entry.text().is_empty() {
            self.show_search_panel(false);
            return;
        }
        let insert_mark = self.buffer.get_insert();
        let mut iter = self.buffer.iter_at_mark(&insert_mark);
        if forward {
            if let Some((_, end)) = self.buffer.selection_bounds() {
                iter = end;
            }
        } else if let Some((start, _)) = self.buffer.selection_bounds() {
            iter = start;
        }

        let result = if forward {
            self.search_context.forward(&iter)
        } else {
            self.search_context.backward(&iter)
        };

        if let Some((match_start, match_end, wrapped)) = result {
            self.buffer.select_range(&match_start, &match_end);
            let view = self.document.view();
            let mut scroll_iter = match_start.clone();
            view.scroll_to_iter(&mut scroll_iter, 0.1, false, 0.0, 0.0);
            if wrapped {
                self.status_label.set_text("Wrapped search");
            } else {
                self.status_label.set_text("");
            }
        } else {
            self.status_label.set_text("No matches");
        }
    }

    pub(super) fn replace_current(&self, advance: bool) {
        if self.search_entry.text().is_empty() {
            self.show_search_panel(false);
            return;
        }
        if self.buffer.selection_bounds().is_none() {
            self.find_next_match(true);
        }
        if let Some((mut start, mut end)) = self.buffer.selection_bounds() {
            let replacement = self.replace_entry.text();
            match self
                .search_context
                .replace(&mut start, &mut end, replacement.as_str())
            {
                Ok(_) => {
                    self.update_search_feedback();
                    if advance {
                        self.find_next_match(true);
                    }
                }
                Err(err) => {
                    self.status_label
                        .set_text(&format!("Replace failed: {}", err));
                }
            }
        }
    }

    pub(super) fn replace_all(&self) {
        if self.search_entry.text().is_empty() {
            self.show_search_panel(false);
            return;
        }
        let replacement = self.replace_entry.text();
        let mut iter = self.buffer.start_iter();
        let mut count = 0;
        self.buffer.begin_user_action();
        while let Some((mut start, mut end, _)) = self.search_context.forward(&iter) {
            match self
                .search_context
                .replace(&mut start, &mut end, replacement.as_str())
            {
                Ok(_) => {
                    iter = end;
                    count += 1;
                }
                Err(err) => {
                    self.status_label
                        .set_text(&format!("Replace failed: {}", err));
                    break;
                }
            }
        }
        self.buffer.end_user_action();
        self.update_search_feedback();
        self.status_label
            .set_text(&format!("Replaced {} matches", count));
    }

    pub(super) fn show_search_panel(&self, focus_replace: bool) {
        if !self.search_revealer.reveals_child() {
            self.search_revealer.set_reveal_child(true);
        }
        if self.search_entry.text().is_empty() {
            if let Some((start, end)) = self.buffer.selection_bounds() {
                let selection = self.buffer.text(&start, &end, true);
                if !selection.is_empty() {
                    self.search_entry.set_text(&selection);
                    self.search_entry.select_region(0, -1);
                }
            }
        }
        if focus_replace {
            self.replace_entry.grab_focus();
        } else {
            self.search_entry.grab_focus();
        }
        self.update_search_pattern();
    }

    pub(super) fn hide_search_panel(&self) {
        self.search_revealer.set_reveal_child(false);
        self.window().grab_focus();
    }
}
