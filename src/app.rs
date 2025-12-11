use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk4::gdk;
use gtk4::glib::{self, ControlFlow, Propagation};
use gtk4::prelude::*;
use gtk4::{self as gtk};
use libadwaita as adw;
use sourceview5::{SearchContext, SearchSettings, prelude::*};
use uuid::Uuid;

use anyhow::Result;

use crate::document::{Document, derive_display_name};
use crate::paths::AppPaths;
use crate::settings::Settings;
use crate::state_store::WindowState;

pub fn build_ui(application: &adw::Application) -> Result<()> {
    let paths = AppPaths::initialize()?;
    let settings = Settings::load(&paths)?;

    let document = Document::new();
    let buffer = document.buffer();
    let view = document.view();

    let window_state = WindowState::load(&paths).unwrap_or_else(|err| {
        log::warn!("Failed to load window state: {err:?}");
        WindowState::default()
    });

    let header = adw::HeaderBar::builder()
        .title_widget(&gtk::Label::new(Some("Ghostpad")))
        .build();
    let new_btn = gtk::Button::from_icon_name("document-new-symbolic");
    new_btn.set_tooltip_text(Some("New document"));
    let open_btn = gtk::Button::from_icon_name("document-open-symbolic");
    open_btn.set_tooltip_text(Some("Open…"));
    let save_btn = gtk::Button::from_icon_name("document-save-symbolic");
    save_btn.set_tooltip_text(Some("Save"));
    let save_as_btn = gtk::Button::from_icon_name("document-save-as-symbolic");
    save_as_btn.set_tooltip_text(Some("Save As…"));
    header.pack_start(&new_btn);
    header.pack_start(&open_btn);
    header.pack_end(&save_as_btn);
    header.pack_end(&save_btn);

    let scroller = gtk::ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&view)
        .build();

    let search_settings = SearchSettings::new();
    search_settings.set_wrap_around(true);
    let search_context = SearchContext::new(&buffer, Some(&search_settings));
    search_context.set_highlight(false);

    let search_entry = gtk::Entry::builder()
        .placeholder_text("Find…")
        .hexpand(true)
        .build();
    let replace_entry = gtk::Entry::builder()
        .placeholder_text("Replace with…")
        .hexpand(true)
        .build();

    let case_toggle = gtk::ToggleButton::with_label("Aa");
    case_toggle.set_tooltip_text(Some("Match case"));
    let word_toggle = gtk::ToggleButton::with_label("W");
    word_toggle.set_tooltip_text(Some("Whole word"));
    let regex_toggle = gtk::ToggleButton::with_label(".*");
    regex_toggle.set_tooltip_text(Some("Regex"));

    let prev_btn = gtk::Button::from_icon_name("go-up-symbolic");
    prev_btn.set_tooltip_text(Some("Find previous"));
    let next_btn = gtk::Button::from_icon_name("go-down-symbolic");
    next_btn.set_tooltip_text(Some("Find next"));
    let replace_btn = gtk::Button::with_label("Replace");
    let replace_all_btn = gtk::Button::with_label("Replace All");
    let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
    close_btn.set_tooltip_text(Some("Close search"));

    let match_label = gtk::Label::new(Some("0 matches"));
    match_label.add_css_class("dim-label");

    let search_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    search_row.append(&search_entry);
    search_row.append(&case_toggle);
    search_row.append(&word_toggle);
    search_row.append(&regex_toggle);
    search_row.append(&prev_btn);
    search_row.append(&next_btn);
    search_row.append(&match_label);
    search_row.append(&close_btn);

    let replace_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    replace_row.append(&replace_entry);
    replace_row.append(&replace_btn);
    replace_row.append(&replace_all_btn);

    let search_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    search_box.append(&search_row);
    search_box.append(&replace_row);

    let search_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideDown)
        .reveal_child(false)
        .child(&search_box)
        .build();

    let status_label = gtk::Label::new(Some("Ready"));
    status_label.set_xalign(0.0);
    let cursor_label = gtk::Label::new(Some("Ln 1, Col 1"));
    let autosave_label = gtk::Label::new(None);

    let status_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .margin_start(12)
        .margin_end(12)
        .margin_top(4)
        .margin_bottom(4)
        .build();
    status_box.append(&status_label);
    status_box.append(&cursor_label);
    status_box.append(&autosave_label);

    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    root.append(&scroller);
    root.append(&search_revealer);
    root.append(&status_box);

    let chrome = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    chrome.append(&header);
    chrome.append(&root);

    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("Ghostpad")
        .default_width(window_state.width)
        .default_height(window_state.height)
        .content(&chrome)
        .build();

    let state = Rc::new(AppState {
        window: window.clone(),
        document,
        buffer,
        file_path: RefCell::new(None),
        status_label,
        cursor_label,
        autosave_label,
        search_revealer: search_revealer.clone(),
        search_entry: search_entry.clone(),
        replace_entry: replace_entry.clone(),
        match_label: match_label.clone(),
        search_settings: search_settings.clone(),
        search_context: search_context.clone(),
        paths,
        settings: RefCell::new(settings),
        window_state: RefCell::new(window_state),
        autosave_source: RefCell::new(None),
        session_token: Uuid::new_v4().to_string(),
    });

    state.initialize();

    {
        let state = Rc::clone(&state);
        search_entry.connect_changed(move |_| {
            state.update_search_pattern();
        });
    }

    {
        let state = Rc::clone(&state);
        search_entry.connect_activate(move |_| {
            state.find_next_match(true);
        });
    }

    {
        let state = Rc::clone(&state);
        replace_entry.connect_activate(move |_| {
            state.replace_current(true);
        });
    }

    {
        let state = Rc::clone(&state);
        case_toggle.connect_toggled(move |btn| {
            state.search_settings.set_case_sensitive(btn.is_active());
            state.update_search_pattern();
        });
    }

    {
        let state = Rc::clone(&state);
        word_toggle.connect_toggled(move |btn| {
            state
                .search_settings
                .set_at_word_boundaries(btn.is_active());
            state.update_search_pattern();
        });
    }

    {
        let state = Rc::clone(&state);
        regex_toggle.connect_toggled(move |btn| {
            state.search_settings.set_regex_enabled(btn.is_active());
            state.update_search_pattern();
        });
    }

    {
        let state = Rc::clone(&state);
        prev_btn.connect_clicked(move |_| state.find_next_match(false));
    }

    {
        let state = Rc::clone(&state);
        next_btn.connect_clicked(move |_| state.find_next_match(true));
    }

    {
        let state = Rc::clone(&state);
        replace_btn.connect_clicked(move |_| state.replace_current(true));
    }

    {
        let state = Rc::clone(&state);
        replace_all_btn.connect_clicked(move |_| state.replace_all());
    }

    {
        let state = Rc::clone(&state);
        close_btn.connect_clicked(move |_| state.hide_search_panel());
    }

    let key_controller = gtk::EventControllerKey::new();
    {
        let state = Rc::clone(&state);
        key_controller.connect_key_pressed(move |_, key, _, modifier| {
            let ctrl = modifier.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = modifier.contains(gdk::ModifierType::SHIFT_MASK);
            if key == gdk::Key::Escape && state.search_revealer.reveals_child() {
                state.hide_search_panel();
                return Propagation::Stop;
            }
            if ctrl && shift && (key == gdk::Key::F || key == gdk::Key::f) {
                state.show_search_panel(true);
                return Propagation::Stop;
            }
            if ctrl {
                match key {
                    gdk::Key::f | gdk::Key::F => {
                        state.show_search_panel(false);
                        return Propagation::Stop;
                    }
                    gdk::Key::g | gdk::Key::G => {
                        state.show_goto_line_dialog();
                        return Propagation::Stop;
                    }
                    _ => {}
                }
            }
            if key == gdk::Key::F3 {
                if shift {
                    state.find_next_match(false);
                } else {
                    state.find_next_match(true);
                }
                return Propagation::Stop;
            }
            Propagation::Proceed
        });
    }
    window.add_controller(key_controller);

    state.update_search_pattern();

    {
        let state = Rc::clone(&state);
        window.connect_close_request(move |_| {
            state.persist_window_state();
            Propagation::Proceed
        });
    }

    {
        let state = Rc::clone(&state);
        new_btn.connect_clicked(move |_| {
            if let Err(err) = state.new_document() {
                state.present_error("New document failed", &err.to_string());
            }
        });
    }

    {
        let state = Rc::clone(&state);
        open_btn.connect_clicked(move |_| {
            state.open_document_dialog();
        });
    }

    {
        let state = Rc::clone(&state);
        save_btn.connect_clicked(move |_| {
            state.save_action();
        });
    }

    {
        let state = Rc::clone(&state);
        save_as_btn.connect_clicked(move |_| {
            state.save_as_dialog();
        });
    }

    window.present();
    Ok(())
}

struct AppState {
    window: adw::ApplicationWindow,
    document: Rc<Document>,
    buffer: sourceview5::Buffer,
    file_path: RefCell<Option<PathBuf>>,
    status_label: gtk::Label,
    cursor_label: gtk::Label,
    autosave_label: gtk::Label,
    search_revealer: gtk::Revealer,
    search_entry: gtk::Entry,
    replace_entry: gtk::Entry,
    match_label: gtk::Label,
    search_settings: SearchSettings,
    search_context: SearchContext,
    paths: AppPaths,
    settings: RefCell<Settings>,
    window_state: RefCell<WindowState>,
    autosave_source: RefCell<Option<glib::SourceId>>,
    session_token: String,
}

impl AppState {
    fn initialize(self: &Rc<Self>) {
        self.update_title();
        self.update_cursor_label();
        self.hook_buffer_signals();
        self.restart_autosave();
    }

    fn hook_buffer_signals(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.buffer.connect_changed(move |_| {
            if let Some(state) = weak.upgrade() {
                state.update_title();
            }
        });

        let weak_cursor = Rc::downgrade(self);
        self.buffer.connect_mark_set(move |_buf, _iter, mark| {
            if mark.name().as_deref() == Some("insert") {
                if let Some(state) = weak_cursor.upgrade() {
                    state.update_cursor_label();
                }
            }
        });

        let weak_modified = Rc::downgrade(self);
        self.buffer.connect_modified_changed(move |buffer| {
            if let Some(state) = weak_modified.upgrade() {
                if !buffer.is_modified() {
                    state.update_title();
                }
            }
        });
    }

    fn new_document(&self) -> anyhow::Result<()> {
        self.document.clear();
        self.file_path.replace(None);
        self.update_title();
        Ok(())
    }

    fn open_document_dialog(self: &Rc<Self>) {
        let dialog = gtk::FileChooserDialog::builder()
            .title("Open File")
            .transient_for(&self.window)
            .modal(true)
            .action(gtk::FileChooserAction::Open)
            .build();
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Open", gtk::ResponseType::Accept);
        Self::attach_file_filters(&dialog);
        let weak = Rc::downgrade(self);
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                if let Some(state) = weak.upgrade() {
                    if let Some(file) = dialog.file() {
                        if let Some(path) = file.path() {
                            match state.document.load_from_path(&path) {
                                Ok(_) => {
                                    state.file_path.replace(Some(path));
                                    state.update_title();
                                }
                                Err(err) => state.present_error("Failed to open", &err.to_string()),
                            }
                        } else {
                            state.present_error(
                                "Unsupported file",
                                "Location is not on the local filesystem",
                            );
                        }
                    }
                }
            }
            dialog.close();
        });
        dialog.show();
    }

    fn save_action(self: &Rc<Self>) {
        if self.file_path.borrow().is_some() {
            if let Err(err) = self.write_current_file() {
                self.present_error("Save failed", &err.to_string());
            }
        } else {
            self.save_as_dialog();
        }
    }

    fn write_current_file(&self) -> anyhow::Result<()> {
        let path = self
            .file_path
            .borrow()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No file selected"))?;
        self.document.save_to_path(&path)?;
        self.update_title();
        Ok(())
    }

    fn save_as_dialog(self: &Rc<Self>) {
        let dialog = gtk::FileChooserDialog::builder()
            .title("Save File As")
            .transient_for(&self.window)
            .modal(true)
            .action(gtk::FileChooserAction::Save)
            .build();
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Save", gtk::ResponseType::Accept);
        Self::attach_file_filters(&dialog);
        let weak = Rc::downgrade(self);
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                if let Some(state) = weak.upgrade() {
                    if let Some(file) = dialog.file() {
                        if let Some(path) = file.path() {
                            match state.document.save_to_path(&path) {
                                Ok(_) => {
                                    state.file_path.replace(Some(path));
                                    state.update_title();
                                    state.run_autosave();
                                }
                                Err(err) => state.present_error("Failed to save", &err.to_string()),
                            }
                        } else {
                            state.present_error(
                                "Unsupported file",
                                "Location is not on the local filesystem",
                            );
                        }
                    }
                }
            }
            dialog.close();
        });
        dialog.show();
    }

    fn restart_autosave(self: &Rc<Self>) {
        if let Some(source) = self.autosave_source.borrow_mut().take() {
            source.remove();
        }
        let interval = self.settings.borrow().autosave_interval_secs;
        if interval == 0 {
            self.autosave_label.set_text("Autosave: Off");
            return;
        }
        self.autosave_label
            .set_text(&format!("Autosave: every {}s", interval));
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

    fn run_autosave(&self) {
        if !self.buffer.is_modified() {
            return;
        }
        match self.write_autosave_file() {
            Ok(timestamp) => self
                .autosave_label
                .set_text(&format!("Autosave @ {}", timestamp)),
            Err(err) => {
                log::warn!("Autosave error: {err:?}");
                self.autosave_label.set_text("Autosave error");
            }
        }
    }

    fn write_autosave_file(&self) -> anyhow::Result<String> {
        let data = self.document.current_text();
        let file = self.autosave_path();
        let temp = file.with_extension("tmp");
        fs::write(&temp, data)?;
        fs::rename(&temp, &file)?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        Ok(format!("{}s", ts))
    }

    fn autosave_path(&self) -> PathBuf {
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

    fn update_title(&self) {
        let name = derive_display_name(&self.file_path.borrow());
        let marker = if self.buffer.is_modified() { "*" } else { "" };
        self.window
            .set_title(Some(&format!("Ghostpad — {name}{marker}")));
        self.status_label.set_text(&format!(
            "{}{}",
            name,
            if marker.is_empty() {
                ""
            } else {
                " • Unsaved"
            }
        ));
    }

    fn update_cursor_label(&self) {
        let iter = self.buffer.iter_at_offset(self.buffer.cursor_position());
        let line = iter.line() + 1;
        let col = iter.line_offset() + 1;
        self.cursor_label.set_text(&format!("Ln {line}, Col {col}"));
    }

    fn present_error(&self, heading: &str, body: &str) {
        let dialog = gtk::MessageDialog::builder()
            .transient_for(&self.window)
            .modal(true)
            .buttons(gtk::ButtonsType::Ok)
            .text(heading)
            .secondary_text(body)
            .build();
        dialog.connect_response(|dialog, _| dialog.close());
        dialog.show();
    }

    fn show_search_panel(&self, focus_replace: bool) {
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

    fn hide_search_panel(&self) {
        self.search_revealer.set_reveal_child(false);
        self.window.grab_focus();
    }

    fn update_search_pattern(&self) {
        let pattern = self.search_entry.text();
        if pattern.is_empty() {
            self.search_settings.set_search_text(None::<&str>);
        } else {
            self.search_settings.set_search_text(Some(pattern.as_str()));
        }
        self.search_context.set_highlight(!pattern.is_empty());
        self.update_search_feedback();
    }

    fn update_search_feedback(&self) {
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

    fn find_next_match(&self, forward: bool) {
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

    fn replace_current(&self, advance: bool) {
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

    fn replace_all(&self) {
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

    fn persist_window_state(&self) {
        let width = self.window.width();
        let height = self.window.height();
        let mut store = self.window_state.borrow_mut();
        store.width = width.max(400);
        store.height = height.max(300);
        if let Err(err) = store.save(&self.paths) {
            log::warn!("Failed to save window state: {err:?}");
        }
    }

    fn show_goto_line_dialog(self: &Rc<Self>) {
        let dialog = gtk::Dialog::builder()
            .transient_for(&self.window)
            .modal(true)
            .title("Go to Line")
            .build();
        dialog.set_transient_for(Some(&self.window));
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Go", gtk::ResponseType::Accept);
        dialog.set_default_response(gtk::ResponseType::Accept);

        let entry = gtk::Entry::builder()
            .placeholder_text("Line number")
            .input_purpose(gtk::InputPurpose::Digits)
            .activates_default(true)
            .build();
        entry.set_margin_top(12);
        entry.set_margin_bottom(12);
        entry.set_margin_start(12);
        entry.set_margin_end(12);
        dialog.content_area().append(&entry);
        entry.grab_focus();

        let weak = Rc::downgrade(self);
        let entry_clone = entry.clone();
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                if let Some(state) = weak.upgrade() {
                    let text = entry_clone.text();
                    if let Ok(mut line) = text.trim().parse::<i32>() {
                        if line <= 0 {
                            line = 1;
                        }
                        let total = state.buffer.line_count().max(1);
                        if line > total {
                            line = total;
                        }
                        if let Some(mut iter) = state.buffer.iter_at_line(line - 1) {
                            state.buffer.place_cursor(&iter);
                            let view = state.document.view();
                            view.scroll_to_iter(&mut iter, 0.1, false, 0.0, 0.0);
                            state
                                .status_label
                                .set_text(&format!("Line {} of {}", line, total));
                        }
                    } else {
                        state.status_label.set_text("Enter a valid line number");
                    }
                }
            }
            dialog.close();
        });

        dialog.show();
    }

    fn attach_file_filters(dialog: &gtk::FileChooserDialog) {
        let text_filter = gtk::FileFilter::new();
        text_filter.set_name(Some("Text files"));
        text_filter.add_mime_type("text/plain");
        text_filter.add_pattern("*.txt");
        dialog.add_filter(&text_filter);

        let md_filter = gtk::FileFilter::new();
        md_filter.set_name(Some("Markdown"));
        md_filter.add_mime_type("text/markdown");
        md_filter.add_pattern("*.md");
        md_filter.add_pattern("*.markdown");
        dialog.add_filter(&md_filter);

        let all_filter = gtk::FileFilter::new();
        all_filter.set_name(Some("All files"));
        all_filter.add_pattern("*");
        dialog.add_filter(&all_filter);
        dialog.set_filter(&text_filter);
    }
}
