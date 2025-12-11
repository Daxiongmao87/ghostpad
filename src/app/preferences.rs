use gtk4::{self as gtk};
use libadwaita::prelude::*;
use libadwaita::{self as adw};

use crate::llm::{GpuDevice, LlmSettings, ProviderKind};
use crate::settings::Settings;

pub(super) struct PreferencesUi {
    pub window: adw::PreferencesWindow,
    pub autosave_combo: adw::ComboRow,
    pub autosave_idle_switch: gtk::Switch,
    pub llm_provider_combo: adw::ComboRow,
    pub llm_endpoint_row: adw::ActionRow,
    pub llm_endpoint_entry: gtk::Entry,
    pub override_model_switch: gtk::Switch,
    pub llm_model_row: adw::ActionRow,
    pub llm_model_entry: gtk::Entry,
    pub gpu_combo: adw::ComboRow,
    pub gpu_model_entry: gtk::Entry,
    pub cpu_model_entry: gtk::Entry,
    pub whitespace_switch: gtk::Switch,
    pub wrap_switch: gtk::Switch,
}

pub(super) fn build_preferences(
    parent: &adw::ApplicationWindow,
    autosave_options: &[(u64, &'static str)],
    settings: &Settings,
    gpus: &[GpuDevice],
) -> PreferencesUi {
    let labels: Vec<&str> = autosave_options.iter().map(|(_, label)| *label).collect();
    let string_list = gtk::StringList::new(labels.as_slice());

    let autosave_combo = adw::ComboRow::builder().title("Autosave frequency").build();
    autosave_combo.set_model(Some(&string_list));
    let initial_index = autosave_options
        .iter()
        .position(|(secs, _)| *secs == settings.autosave_interval_secs)
        .unwrap_or(0);
    autosave_combo.set_selected(initial_index as u32);

    let autosave_idle_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(settings.autosave_idle_only)
        .build();
    let autosave_idle_row = adw::ActionRow::builder()
        .title("Only autosave when idle")
        .subtitle("Skip background saves while you are actively typing")
        .build();
    autosave_idle_row.add_suffix(&autosave_idle_switch);
    autosave_idle_row.set_activatable_widget(Some(&autosave_idle_switch));

    let autosave_group = adw::PreferencesGroup::builder()
        .title("Autosave")
        .description("Configure automatic save cadence and behavior.")
        .build();
    autosave_group.add(&autosave_combo);
    autosave_group.add(&autosave_idle_row);

    let autosave_page = adw::PreferencesPage::builder().title("Autosave").build();
    autosave_page.add(&autosave_group);

    let (editor_page, whitespace_switch, wrap_switch) = build_editor_page(settings);
    let (
        llm_page,
        llm_provider_combo,
        llm_endpoint_row,
        llm_endpoint_entry,
        override_model_switch,
        llm_model_row,
        llm_model_entry,
        gpu_combo,
        gpu_model_entry,
        cpu_model_entry,
    ) = build_llm_page(&settings.llm, gpus);
    let theming_page = build_theming_page();
    let shortcuts_page = build_shortcuts_page();

    let window = adw::PreferencesWindow::builder()
        .title("Preferences")
        .transient_for(parent)
        .modal(true)
        .build();
    window.add(&autosave_page);
    window.add(&editor_page);
    window.add(&llm_page);
    window.add(&theming_page);
    window.add(&shortcuts_page);
    PreferencesUi {
        window,
        autosave_combo,
        autosave_idle_switch,
        llm_provider_combo,
        llm_endpoint_row,
        llm_endpoint_entry,
        override_model_switch,
        llm_model_row,
        llm_model_entry,
        gpu_combo,
        gpu_model_entry,
        cpu_model_entry,
        whitespace_switch,
        wrap_switch,
    }
}

fn build_editor_page(settings: &Settings) -> (adw::PreferencesPage, gtk::Switch, gtk::Switch) {
    let page = adw::PreferencesPage::builder().title("Editor").build();
    let group = adw::PreferencesGroup::builder()
        .title("Editor basics")
        .description("Configure text display and behavior.")
        .build();

    let font_row = adw::ActionRow::builder()
        .title("Font")
        .subtitle("System default (click to change)")
        .build();
    font_row.add_suffix(&gtk::Button::with_label("Select…"));
    group.add(&font_row);

    let whitespace_row = adw::ActionRow::builder()
        .title("Show whitespace")
        .subtitle("Display tabs/spaces as faint markers")
        .build();
    let whitespace_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(settings.show_whitespace)
        .build();
    whitespace_row.add_suffix(&whitespace_switch);
    whitespace_row.set_activatable_widget(Some(&whitespace_switch));
    group.add(&whitespace_row);

    let wrap_row = adw::ActionRow::builder()
        .title("Soft wrap lines")
        .subtitle("Wrap long lines to the view width")
        .build();
    let wrap_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(settings.wrap_text)
        .build();
    wrap_row.add_suffix(&wrap_switch);
    wrap_row.set_activatable_widget(Some(&wrap_switch));
    group.add(&wrap_row);

    page.add(&group);
    (page, whitespace_switch, wrap_switch)
}

fn build_llm_page(
    llm: &LlmSettings,
    gpus: &[GpuDevice],
) -> (
    adw::PreferencesPage,
    adw::ComboRow,
    adw::ActionRow,
    gtk::Entry,
    gtk::Switch,
    adw::ActionRow,
    gtk::Entry,
    adw::ComboRow,
    gtk::Entry,
    gtk::Entry,
) {
    let page = adw::PreferencesPage::builder().title("LLM").build();

    let provider_group = adw::PreferencesGroup::builder()
        .title("Providers")
        .description("Select which completion backend Ghostpad uses.")
        .build();

    let provider_names: Vec<&'static str> = PROVIDERS.iter().map(|(_, name)| *name).collect();
    let provider_list = gtk::StringList::new(provider_names.as_slice());
    let provider_row = adw::ComboRow::builder()
        .title("Completion provider")
        .model(&provider_list)
        .selected(provider_index(&llm.provider) as u32)
        .build();
    provider_group.add(&provider_row);

    let endpoint_row = adw::ActionRow::builder()
        .title("Endpoint URL")
        .subtitle("https://api.openai.com/v1")
        .build();
    let endpoint_entry = gtk::Entry::builder()
        .hexpand(true)
        .text(&llm.endpoint)
        .build();
    endpoint_row.add_suffix(&endpoint_entry);
    endpoint_row.set_visible(llm.provider != ProviderKind::Local);
    provider_group.add(&endpoint_row);

    let local_group = adw::PreferencesGroup::builder()
        .title("Local models")
        .description("Configure llama.cpp paths when using local inference.")
        .build();

    let override_model_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(llm.override_model_path)
        .build();
    let override_model_row = adw::ActionRow::builder()
        .title("Override model path")
        .subtitle("Manually specify a GGUF file instead of using defaults")
        .build();
    override_model_row.add_suffix(&override_model_switch);
    override_model_row.set_activatable_widget(Some(&override_model_switch));
    local_group.add(&override_model_row);

    let model_row = adw::ActionRow::builder()
        .title("Model path")
        .subtitle("GGUF file on disk")
        .build();
    let model_entry = gtk::Entry::builder()
        .hexpand(true)
        .text(&llm.local_model_path)
        .sensitive(llm.override_model_path)
        .build();
    model_row.add_suffix(&model_entry);
    local_group.add(&model_row);

    let gpu_names: Vec<String> = std::iter::once("CPU Only".to_string())
        .chain(gpus.iter().map(|g| g.name.clone()))
        .collect();
    let gpu_strings: Vec<&str> = gpu_names.iter().map(|s| s.as_str()).collect();
    let gpu_list = gtk::StringList::new(gpu_strings.as_slice());

    let gpu_combo = adw::ComboRow::builder()
        .title("Preferred device")
        .subtitle("Select GPU or CPU-only inference")
        .model(&gpu_list)
        .build();

    let selected_idx = if llm.force_cpu_only {
        0
    } else if let Some(ref device) = llm.preferred_device {
        gpus.iter()
            .position(|g| &g.id == device)
            .map(|i| i + 1)
            .unwrap_or(0)
    } else {
        0
    };
    gpu_combo.set_selected(selected_idx as u32);
    local_group.add(&gpu_combo);

    let gpu_model_row = adw::ActionRow::builder()
        .title("GPU model")
        .subtitle("Default GGUF for GPU inference")
        .build();
    let gpu_model_entry = gtk::Entry::builder()
        .hexpand(true)
        .text(&llm.default_gpu_model)
        .build();
    gpu_model_row.add_suffix(&gpu_model_entry);
    local_group.add(&gpu_model_row);

    let cpu_model_row = adw::ActionRow::builder()
        .title("CPU model")
        .subtitle("Default GGUF for CPU-only inference")
        .build();
    let cpu_model_entry = gtk::Entry::builder()
        .hexpand(true)
        .text(&llm.default_cpu_model)
        .build();
    cpu_model_row.add_suffix(&cpu_model_entry);
    local_group.add(&cpu_model_row);

    let credentials_group = adw::PreferencesGroup::builder()
        .title("Credentials")
        .description("Tokens are stored via libsecret/KWallet (coming soon)")
        .build();
    let token_row = adw::ActionRow::builder()
        .title("API token")
        .subtitle("Not stored in config yet")
        .build();
    let token_entry = gtk::Entry::builder()
        .visibility(false)
        .hexpand(true)
        .build();
    token_row.add_suffix(&token_entry);
    credentials_group.add(&token_row);

    let test_row = adw::ActionRow::builder()
        .title("Connection test")
        .subtitle("Send a short verify request")
        .build();
    let test_button = gtk::Button::with_label("Test");
    test_row.add_suffix(&test_button);
    test_row.set_activatable_widget(Some(&test_button));
    credentials_group.add(&test_row);

    page.add(&provider_group);
    page.add(&local_group);
    page.add(&credentials_group);
    (
        page,
        provider_row,
        endpoint_row,
        endpoint_entry,
        override_model_switch,
        model_row,
        model_entry,
        gpu_combo,
        gpu_model_entry,
        cpu_model_entry,
    )
}

const PROVIDERS: &[(ProviderKind, &str)] = &[
    (ProviderKind::OpenAI, "OpenAI"),
    (ProviderKind::Gemini, "Gemini"),
    (ProviderKind::Local, "Local llama.cpp"),
];

pub(super) fn provider_index(kind: &ProviderKind) -> usize {
    PROVIDERS
        .iter()
        .position(|(k, _)| k == kind)
        .unwrap_or(0)
}

pub(super) fn provider_from_index(idx: u32) -> ProviderKind {
    PROVIDERS
        .get(idx as usize)
        .map(|(kind, _)| *kind)
        .unwrap_or(ProviderKind::OpenAI)
}

fn build_theming_page() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder().title("Theming").build();
    let group = adw::PreferencesGroup::builder()
        .title("Appearance")
        .description("Follow system defaults; overrides coming soon.")
        .build();
    let theme_switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
    let theme_row = adw::ActionRow::builder()
        .title("Follow system color scheme")
        .subtitle("Automatically mirror light/dark setting")
        .build();
    theme_row.add_suffix(&theme_switch);
    theme_row.set_activatable_widget(Some(&theme_switch));
    group.add(&theme_row);

    let contrast_switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
    let contrast_row = adw::ActionRow::builder()
        .title("High contrast mode")
        .subtitle("Increase contrast for accessibility")
        .build();
    contrast_row.add_suffix(&contrast_switch);
    contrast_row.set_activatable_widget(Some(&contrast_switch));
    group.add(&contrast_row);
    page.add(&group);
    page
}

fn build_shortcuts_page() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder().title("Shortcuts").build();
    let group = adw::PreferencesGroup::builder()
        .title("Keyboard bindings")
        .description("Import/export planned for later milestone.")
        .build();
    let selector = adw::ActionRow::builder()
        .title("Shortcut presets")
        .subtitle("Default (customization coming soon)")
        .build();
    let import_button = gtk::Button::with_label("Import…");
    let export_button = gtk::Button::with_label("Export…");
    selector.add_suffix(&import_button);
    selector.add_suffix(&export_button);
    group.add(&selector);
    page.add(&group);
    page
}
