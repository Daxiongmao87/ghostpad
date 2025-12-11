use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use adw::prelude::*;
use gtk4::gdk;
use gtk4::gio;
use gtk4::glib::{self, Propagation};
use gtk4::prelude::*;
use gtk4::{self as gtk};
use libadwaita as adw;
use sourceview5::{SearchContext, SearchSettings, prelude::*};
use uuid::Uuid;

use anyhow::Result;

use crate::document::{Document, derive_display_name};
use crate::llm::{GpuDevice, LlmManager, ProviderKind};
use crate::paths::AppPaths;
use crate::settings::Settings;
use crate::state_store::WindowState;

use super::autosave::CUSTOM_AUTOSAVE_SENTINEL;
use super::preferences::{self, PreferencesUi};

pub fn build_ui(application: &adw::Application) -> Result<()> {
    let paths = AppPaths::initialize()?;
    let settings = Settings::load(&paths)?;
    let llm_manager = RefCell::new(LlmManager::new(settings.llm.clone()));

    let document = Document::new();
    let buffer = document.buffer();
    let view = document.view();

    let window_state = WindowState::load(&paths).unwrap_or_else(|err| {
        log::warn!("Failed to load window state: {err:?}");
        WindowState::default()
    });
    let initial_recent: Vec<PathBuf> = settings
        .recent_files
        .iter()
        .filter_map(|s| {
            if s.is_empty() {
                None
            } else {
                Some(PathBuf::from(s))
            }
        })
        .collect();

    let header = adw::HeaderBar::builder()
        .title_widget(&gtk::Label::new(Some("Ghostpad")))
        .build();
    let new_btn = gtk::Button::from_icon_name("document-new-symbolic");
    new_btn.set_tooltip_text(Some("New document"));
    let open_btn = gtk::Button::from_icon_name("document-open-symbolic");
    open_btn.set_tooltip_text(Some("Open…"));
    let recent_list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    let recent_popover = gtk::Popover::builder()
        .has_arrow(true)
        .child(&recent_list)
        .build();
    let recent_button = gtk::MenuButton::builder()
        .icon_name("document-open-recent-symbolic")
        .tooltip_text("Recent files")
        .popover(&recent_popover)
        .build();
    let save_btn = gtk::Button::from_icon_name("document-save-symbolic");
    save_btn.set_tooltip_text(Some("Save"));
    let save_as_btn = gtk::Button::from_icon_name("document-save-as-symbolic");
    save_as_btn.set_tooltip_text(Some("Save As…"));
    let prefs_button = gtk::Button::from_icon_name("emblem-system-symbolic");
    prefs_button.set_tooltip_text(Some("Preferences"));
    header.pack_start(&new_btn);
    header.pack_start(&open_btn);
    header.pack_start(&recent_button);
    header.pack_end(&prefs_button);
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
    let autosave_options: Vec<(u64, &'static str)> = vec![
        (0, "Off"),
        (15, "Every 15s"),
        (30, "Every 30s"),
        (60, "Every 60s"),
        (300, "Every 5m"),
        (CUSTOM_AUTOSAVE_SENTINEL, "Custom…"),
    ];
    let autosave_list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    for (_, label) in autosave_options.iter() {
        let row = gtk::ListBoxRow::builder().activatable(true).build();
        row.set_selectable(false);
        let text = gtk::Label::new(Some(label));
        text.set_xalign(0.0);
        text.set_margin_start(12);
        text.set_margin_end(12);
        text.set_margin_top(6);
        text.set_margin_bottom(6);
        row.set_child(Some(&text));
        autosave_list.append(&row);
    }
    let autosave_popover = gtk::Popover::builder()
        .has_arrow(true)
        .child(&autosave_list)
        .build();
    let autosave_button = gtk::MenuButton::builder()
        .label("Autosave")
        .popover(&autosave_popover)
        .build();

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
    status_box.append(&autosave_button);
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

    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&chrome));

    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("Ghostpad")
        .default_width(window_state.width)
        .default_height(window_state.height)
        .content(&overlay)
        .build();

    let detected_gpus = LlmManager::detect_gpus();
    let preferences_ui =
        preferences::build_preferences(&window, &autosave_options, &settings, &detected_gpus);

    let state = Rc::new(AppState {
        window: window.clone(),
        toast_overlay: overlay.clone(),
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
        recent_button: recent_button.clone(),
        recent_list: recent_list.clone(),
        recent_entries: RefCell::new(initial_recent),
        autosave_button: autosave_button.clone(),
        autosave_options,
        preferences: preferences_ui,
        llm_manager,
        gpus: detected_gpus,
        paths,
        settings: RefCell::new(settings),
        window_state: RefCell::new(window_state),
        autosave_source: RefCell::new(None),
        file_monitor: RefCell::new(None),
        external_change_pending: Cell::new(false),
        last_edit: RefCell::new(None),
        session_token: Uuid::new_v4().to_string(),
    });

    state.initialize();
    state.refresh_recent_menu();
    state.check_recovery_snapshots();

    {
        let prefs = state.preferences.window.clone();
        prefs_button.connect_clicked(move |_| {
            prefs.present();
        });
    }

    {
        let state = Rc::clone(&state);
        recent_list.connect_row_activated(move |_, row| {
            let idx = row.index();
            if idx < 0 {
                return;
            }
            if let Some(path) = state.recent_entries.borrow().get(idx as usize).cloned() {
                state.confirm_unsaved_then(move |st| {
                    if let Err(err) = st.load_document_from_path(&path) {
                        st.present_error("Failed to open", &err.to_string());
                    }
                });
            }
        });
    }

    {
        let state = Rc::clone(&state);
        search_entry.connect_changed(move |_| {
            state.update_search_pattern();
        });
    }

    {
        let state = Rc::clone(&state);
        autosave_list.connect_row_activated(move |list, row| {
            let idx = row.index();
            if idx < 0 {
                return;
            }
            if let Some((secs, _)) = state.autosave_options.get(idx as usize) {
                if *secs == CUSTOM_AUTOSAVE_SENTINEL {
                    state.prompt_custom_autosave();
                } else {
                    state.set_autosave_interval(*secs);
                }
            }
            list.unselect_all();
        });
    }

    {
        let state = Rc::clone(&state);
        let combo = state.preferences.autosave_combo.clone();
        combo.connect_selected_notify(move |row: &adw::ComboRow| {
            let idx = row.selected() as usize;
            if let Some((secs, _)) = state.autosave_options.get(idx) {
                if *secs == CUSTOM_AUTOSAVE_SENTINEL {
                    state.prompt_custom_autosave();
                } else if *secs != state.settings.borrow().autosave_interval_secs {
                    state.set_autosave_interval(*secs);
                }
            }
        });
    }

    {
        let state = Rc::clone(&state);
        let idle_switch = state.preferences.autosave_idle_switch.clone();
        idle_switch.connect_active_notify(move |switch_widget: &gtk::Switch| {
            let active = switch_widget.is_active();
            if active == state.settings.borrow().autosave_idle_only {
                return;
            }
            state.set_autosave_idle_only(active);
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
        window.connect_close_request(move |win| {
            if !state.buffer.is_modified() {
                state.persist_window_state();
                return Propagation::Proceed;
            }
            let win_clone = win.clone();
            state.confirm_unsaved_then(move |st| {
                st.persist_window_state();
                win_clone.close();
            });
            Propagation::Stop
        });
    }

    {
        let state = Rc::clone(&state);
        new_btn.connect_clicked(move |_| {
            state.confirm_unsaved_then(move |st| {
                if let Err(err) = st.new_document() {
                    st.present_error("New document failed", &err.to_string());
                } else {
                    st.status_label.set_text("New document");
                }
            });
        });
    }

    {
        let state = Rc::clone(&state);
        open_btn.connect_clicked(move |_| {
            state.confirm_unsaved_then(move |st| {
                st.open_document_dialog();
            });
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

pub(super) struct AppState {
    pub(super) window: adw::ApplicationWindow,
    pub(super) toast_overlay: adw::ToastOverlay,
    pub(super) document: Rc<Document>,
    pub(super) buffer: sourceview5::Buffer,
    pub(super) file_path: RefCell<Option<PathBuf>>,
    pub(super) status_label: gtk::Label,
    pub(super) cursor_label: gtk::Label,
    pub(super) autosave_label: gtk::Label,
    pub(super) search_revealer: gtk::Revealer,
    pub(super) search_entry: gtk::Entry,
    pub(super) replace_entry: gtk::Entry,
    pub(super) match_label: gtk::Label,
    pub(super) search_settings: SearchSettings,
    pub(super) search_context: SearchContext,
    pub(super) recent_button: gtk::MenuButton,
    pub(super) recent_list: gtk::ListBox,
    pub(super) recent_entries: RefCell<Vec<PathBuf>>,
    pub(super) autosave_button: gtk::MenuButton,
    pub(super) autosave_options: Vec<(u64, &'static str)>,
    pub(super) preferences: PreferencesUi,
    pub(super) llm_manager: RefCell<LlmManager>,
    pub(super) gpus: Vec<GpuDevice>,
    pub(super) paths: AppPaths,
    pub(super) settings: RefCell<Settings>,
    pub(super) window_state: RefCell<WindowState>,
    pub(super) autosave_source: RefCell<Option<glib::SourceId>>,
    pub(super) file_monitor: RefCell<Option<gio::FileMonitor>>,
    pub(super) external_change_pending: Cell<bool>,
    pub(super) last_edit: RefCell<Option<Instant>>,
    pub(super) session_token: String,
}

impl AppState {
    fn initialize(self: &Rc<Self>) {
        self.update_title();
        self.update_cursor_label();
        self.hook_buffer_signals();
        self.restart_autosave();
        self.sync_preferences_ui();
        self.sync_llm_preferences();
        self.hook_llm_preferences();
    }

    fn hook_buffer_signals(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.buffer.connect_changed(move |_| {
            if let Some(state) = weak.upgrade() {
                state.update_title();
                state.last_edit.replace(Some(Instant::now()));
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

    fn new_document(self: &Rc<Self>) -> anyhow::Result<()> {
        self.document.clear();
        self.file_path.replace(None);
        self.stop_file_monitor();
        self.last_edit.replace(None);
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
                            if let Err(err) = state.load_document_from_path(&path) {
                                state.present_error("Failed to open", &err.to_string());
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

    fn write_current_file(self: &Rc<Self>) -> anyhow::Result<()> {
        let path = self
            .file_path
            .borrow()
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No file selected"))?;
        self.document.save_to_path(&path)?;
        self.remove_autosave_artifacts();
        self.record_recent_file(&path);
        self.watch_active_file();
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
                                    state.file_path.replace(Some(path.clone()));
                                    state.remove_autosave_artifacts();
                                    state.record_recent_file(&path);
                                    state.watch_active_file();
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

    pub(super) fn update_title(&self) {
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

    pub(super) fn present_error(&self, heading: &str, body: &str) {
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

    fn watch_active_file(self: &Rc<Self>) {
        self.stop_file_monitor();
        if let Some(path) = self.file_path.borrow().clone() {
            let file = gio::File::for_path(&path);
            match file.monitor_file(gio::FileMonitorFlags::NONE, None::<&gio::Cancellable>) {
                Ok(monitor) => {
                    let weak = Rc::downgrade(self);
                    monitor.connect_changed(move |_, _, _, event| {
                        if matches!(
                            event,
                            gio::FileMonitorEvent::Changed
                                | gio::FileMonitorEvent::ChangesDoneHint
                                | gio::FileMonitorEvent::Deleted
                        ) {
                            if let Some(state) = weak.upgrade() {
                                state.handle_external_change();
                            }
                        }
                    });
                    self.file_monitor.replace(Some(monitor));
                }
                Err(err) => log::warn!("Failed to watch file: {err:?}"),
            }
        }
    }

    fn stop_file_monitor(&self) {
        self.file_monitor.borrow_mut().take();
        self.external_change_pending.set(false);
    }

    fn handle_external_change(self: &Rc<Self>) {
        if self.external_change_pending.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        let dialog = gtk::MessageDialog::builder()
            .transient_for(&self.window)
            .modal(true)
            .text("File changed on disk")
            .secondary_text("The file was modified outside Ghostpad. Reload it?")
            .build();
        dialog.add_button("Keep My Changes", gtk::ResponseType::Cancel);
        dialog.add_button("Reload", gtk::ResponseType::Accept);
        dialog.connect_response(move |dialog: &gtk::MessageDialog, response| {
            if let Some(state) = weak.upgrade() {
                if response == gtk::ResponseType::Accept {
                    state.reload_from_disk();
                } else {
                    state.external_change_pending.set(false);
                }
            }
            dialog.close();
        });
        dialog.show();
    }

    fn reload_from_disk(self: &Rc<Self>) {
        if let Some(path) = self.file_path.borrow().clone() {
            match self.document.load_from_path(&path) {
                Ok(_) => {
                    self.buffer.set_modified(false);
                    self.update_title();
                    self.status_label.set_text("Reloaded from disk");
                    self.watch_active_file();
                }
                Err(err) => self.present_error("Failed to reload", &err.to_string()),
            }
        }
        self.external_change_pending.set(false);
    }

    fn load_document_from_path(self: &Rc<Self>, path: &Path) -> Result<()> {
        self.remove_autosave_artifacts();
        self.document.load_from_path(path)?;
        self.file_path.replace(Some(path.to_path_buf()));
        self.buffer.set_modified(false);
        self.update_title();
        self.record_recent_file(path);
        self.watch_active_file();
        self.last_edit.replace(None);
        Ok(())
    }

    pub(super) fn show_toast(&self, message: &str) {
        let toast = adw::Toast::new(message);
        self.toast_overlay.add_toast(toast);
    }

    fn confirm_unsaved_then<F>(self: &Rc<Self>, proceed: F)
    where
        F: FnOnce(&Rc<Self>) + 'static,
    {
        if !self.buffer.is_modified() {
            proceed(self);
            return;
        }
        let proceed_cell: Rc<RefCell<Option<Box<dyn FnOnce(&Rc<Self>)>>>> =
            Rc::new(RefCell::new(Some(Box::new(proceed))));
        let dialog = gtk::MessageDialog::builder()
            .transient_for(&self.window)
            .modal(true)
            .text("Unsaved changes")
            .secondary_text("Save your changes before continuing?")
            .build();
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Discard", gtk::ResponseType::Reject);
        dialog.add_button("Save", gtk::ResponseType::Accept);
        let weak = Rc::downgrade(self);
        let proceed_clone = Rc::clone(&proceed_cell);
        dialog.connect_response(move |dialog, response| {
            if let Some(state) = weak.upgrade() {
                match response {
                    gtk::ResponseType::Accept => {
                        state.save_action();
                        if state.buffer.is_modified() {
                            return;
                        }
                    }
                    gtk::ResponseType::Reject => {}
                    _ => {
                        dialog.close();
                        return;
                    }
                }
                if let Some(callback) = proceed_clone.borrow_mut().take() {
                    callback(&state);
                }
            }
            dialog.close();
        });
        dialog.show();
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

    fn sync_llm_preferences(&self) {
        let (provider, idx, endpoint, override_model, model_path, gpu_idx, gpu_model, cpu_model) = {
            let settings = self.settings.borrow();
            let provider = settings.llm.provider;
            let idx = preferences::provider_index(&provider);
            let endpoint = settings.llm.endpoint.clone();
            let override_model = settings.llm.override_model_path;
            let model_path = settings.llm.local_model_path.clone();
            let gpu_idx = if settings.llm.force_cpu_only {
                0
            } else if let Some(ref device) = settings.llm.preferred_device {
                self.gpus
                    .iter()
                    .position(|g| &g.id == device)
                    .map(|i| i + 1)
                    .unwrap_or(0)
            } else {
                0
            };
            let gpu_model = settings.llm.default_gpu_model.clone();
            let cpu_model = settings.llm.default_cpu_model.clone();
            (provider, idx, endpoint, override_model, model_path, gpu_idx, gpu_model, cpu_model)
        };

        self.preferences.llm_provider_combo.set_selected(idx as u32);
        self.preferences.llm_endpoint_row.set_visible(provider != ProviderKind::Local);
        self.preferences.llm_endpoint_entry.set_text(&endpoint);
        self.preferences.override_model_switch.set_active(override_model);
        self.preferences.llm_model_entry.set_sensitive(override_model);
        self.preferences.llm_model_entry.set_text(&model_path);
        self.preferences.gpu_combo.set_selected(gpu_idx as u32);
        self.preferences.gpu_model_entry.set_text(&gpu_model);
        self.preferences.cpu_model_entry.set_text(&cpu_model);
    }

    fn hook_llm_preferences(self: &Rc<Self>) {
        let state = Rc::clone(self);
        self.preferences
            .llm_provider_combo
            .connect_selected_notify(move |row| {
                let provider = preferences::provider_from_index(row.selected());
                state.update_llm_provider(provider);
            });

        let state = Rc::clone(self);
        self.preferences
            .llm_endpoint_entry
            .connect_changed(move |entry| {
                state.update_llm_endpoint(entry.text().to_string());
            });

        let state = Rc::clone(self);
        self.preferences
            .override_model_switch
            .connect_state_set(move |_, active| {
                state.update_override_model(active);
                Propagation::Proceed
            });

        let state = Rc::clone(self);
        self.preferences
            .llm_model_entry
            .connect_changed(move |entry| {
                state.update_llm_local_model(entry.text().to_string());
            });

        let state = Rc::clone(self);
        self.preferences
            .gpu_combo
            .connect_selected_notify(move |row| {
                let idx = row.selected();
                state.update_gpu_selection(idx);
            });

        let state = Rc::clone(self);
        self.preferences
            .gpu_model_entry
            .connect_changed(move |entry| {
                state.update_gpu_model(entry.text().to_string());
            });

        let state = Rc::clone(self);
        self.preferences
            .cpu_model_entry
            .connect_changed(move |entry| {
                state.update_cpu_model(entry.text().to_string());
            });
    }

    fn update_llm_provider(&self, provider: ProviderKind) {
        {
            let mut settings = self.settings.borrow_mut();
            if settings.llm.provider == provider {
                return;
            }
            settings.llm.provider = provider;
        }
        self.save_settings();
        self.llm_manager
            .borrow_mut()
            .update_config(self.settings.borrow().llm.clone());
        self.sync_llm_preferences();
    }

    fn update_llm_endpoint(&self, endpoint: String) {
        {
            let mut settings = self.settings.borrow_mut();
            if settings.llm.endpoint == endpoint {
                return;
            }
            settings.llm.endpoint = endpoint;
        }
        self.save_settings();
        self.llm_manager
            .borrow_mut()
            .update_config(self.settings.borrow().llm.clone());
    }

    fn update_llm_local_model(&self, path: String) {
        {
            let mut settings = self.settings.borrow_mut();
            if settings.llm.local_model_path == path {
                return;
            }
            settings.llm.local_model_path = path;
        }
        self.save_settings();
        self.llm_manager
            .borrow_mut()
            .update_config(self.settings.borrow().llm.clone());
    }

    fn update_override_model(&self, active: bool) {
        {
            let mut settings = self.settings.borrow_mut();
            if settings.llm.override_model_path == active {
                return;
            }
            settings.llm.override_model_path = active;
        }
        self.save_settings();
        self.llm_manager
            .borrow_mut()
            .update_config(self.settings.borrow().llm.clone());
        self.sync_llm_preferences();
    }

    fn update_gpu_selection(&self, idx: u32) {
        {
            let mut settings = self.settings.borrow_mut();
            if idx == 0 {
                settings.llm.force_cpu_only = true;
                settings.llm.preferred_device = None;
            } else {
                settings.llm.force_cpu_only = false;
                let gpu_idx = (idx as usize) - 1;
                if let Some(gpu) = self.gpus.get(gpu_idx) {
                    settings.llm.preferred_device = Some(gpu.id.clone());
                }
            }
        }
        self.save_settings();
        self.llm_manager
            .borrow_mut()
            .update_config(self.settings.borrow().llm.clone());
        self.sync_llm_preferences();
    }

    fn update_gpu_model(&self, model: String) {
        {
            let mut settings = self.settings.borrow_mut();
            if settings.llm.default_gpu_model == model {
                return;
            }
            settings.llm.default_gpu_model = model;
        }
        self.save_settings();
        self.llm_manager
            .borrow_mut()
            .update_config(self.settings.borrow().llm.clone());
    }

    fn update_cpu_model(&self, model: String) {
        {
            let mut settings = self.settings.borrow_mut();
            if settings.llm.default_cpu_model == model {
                return;
            }
            settings.llm.default_cpu_model = model;
        }
        self.save_settings();
        self.llm_manager
            .borrow_mut()
            .update_config(self.settings.borrow().llm.clone());
    }

    fn save_settings(&self) {
        if let Err(err) = self.settings.borrow().save(&self.paths) {
            log::warn!("Failed to save settings: {err:?}");
        }
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
