use gtk4::{self as gtk, glib};
use libadwaita::prelude::*;
use libadwaita::{self as adw};

use crate::llm::{GpuDevice, LlmSettings, ProviderKind};
use crate::settings::Settings;

pub(super) struct PreferencesUi {
    pub window: adw::PreferencesWindow,
    pub autosave_combo: adw::ComboRow,
    pub autosave_idle_switch: gtk::Switch,
    pub llm_provider_combo: adw::ComboRow,
    pub llm_endpoint_row: adw::EntryRow,
    pub override_model_switch: gtk::Switch,
    pub llm_model_row: adw::EntryRow,
    pub gpu_combo: adw::ComboRow,
    pub gpu_model_row: adw::EntryRow,
    pub gpu_download_button: gtk::Button,
    pub cpu_model_row: adw::EntryRow,
    pub cpu_download_button: gtk::Button,
    pub reset_defaults_button: gtk::Button,
    pub max_tokens_spin: gtk::SpinButton,
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

    let autosave_combo = adw::ComboRow::builder().title("Frequence").build();
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
        .title("Idle Only")
        .subtitle("Pause autosave while typing")
        .build();
    autosave_idle_row.add_suffix(&autosave_idle_switch);
    autosave_idle_row.set_activatable_widget(Some(&autosave_idle_switch));

    let autosave_group = adw::PreferencesGroup::builder()
        .title("Behavior")
        .build();
    autosave_group.add(&autosave_combo);
    autosave_group.add(&autosave_idle_row);

    let autosave_page = adw::PreferencesPage::builder()
        .title("Autosave")
        .icon_name("document-save-symbolic")
        .build();
    autosave_page.add(&autosave_group);

    let (editor_page, whitespace_switch, wrap_switch) = build_editor_page(settings);
    let (
        llm_page,
        llm_provider_combo,
        llm_endpoint_row,
        override_model_switch,
        llm_model_row,
        gpu_combo,
        gpu_model_row,
        gpu_download_button,
        cpu_model_row,
        cpu_download_button,
        reset_defaults_button,
        max_tokens_spin,
    ) = build_llm_page(&settings.llm, gpus);
    let theming_page = build_theming_page();
    // Shortcuts page removed for now as it was empty/placeholder

    let window = adw::PreferencesWindow::builder()
        .title("Preferences")
        .transient_for(parent)
        .modal(true)
        .build();
    window.add(&editor_page);
    window.add(&autosave_page);
    window.add(&llm_page);
    window.add(&theming_page);
    
    PreferencesUi {
        window,
        autosave_combo,
        autosave_idle_switch,
        llm_provider_combo,
        llm_endpoint_row,
        override_model_switch,
        llm_model_row,
        gpu_combo,
        gpu_model_row,
        gpu_download_button,
        cpu_model_row,
        cpu_download_button,
        reset_defaults_button,
        max_tokens_spin,
        whitespace_switch,
        wrap_switch,
    }
}

fn build_editor_page(settings: &Settings) -> (adw::PreferencesPage, gtk::Switch, gtk::Switch) {
    let page = adw::PreferencesPage::builder()
        .title("Editor")
        .icon_name("accessories-text-editor-symbolic")
        .build();
    let group = adw::PreferencesGroup::builder()
        .title("Appearance")
        .build();

    // Font selection (simplified placeholder for now, ideally GtkFontDialog)
    let font_row = adw::ActionRow::builder()
        .title("Font")
        .subtitle("System default")
        .build();
    font_row.add_suffix(&gtk::Button::with_label("Select…")); // Keeping simple for now
    group.add(&font_row);

    let whitespace_row = adw::ActionRow::builder()
        .title("Show Whitespace")
        .build();
    let whitespace_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(settings.show_whitespace)
        .build();
    whitespace_row.add_suffix(&whitespace_switch);
    whitespace_row.set_activatable_widget(Some(&whitespace_switch));
    group.add(&whitespace_row);

    let wrap_row = adw::ActionRow::builder()
        .title("Soft Wrap")
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
    adw::EntryRow,
    gtk::Switch,
    adw::EntryRow,
    adw::ComboRow,
    adw::EntryRow,
    gtk::Button,
    adw::EntryRow,
    gtk::Button,
    gtk::Button,
    gtk::SpinButton,
) {
    let page = adw::PreferencesPage::builder()
        .title("AI Assistant")
        .icon_name("sparkles-symbolic")
        .build();

    let provider_group = adw::PreferencesGroup::builder()
        .title("Provider")
        .description("Choose your completion backend.")
        .build();

    let provider_names: Vec<&'static str> = PROVIDERS.iter().map(|(_, name)| *name).collect();
    let provider_list = gtk::StringList::new(provider_names.as_slice());
    let provider_row = adw::ComboRow::builder()
        .title("Service")
        .model(&provider_list)
        .selected(provider_index(&llm.provider) as u32)
        .build();
    provider_group.add(&provider_row);

    let endpoint_row = adw::EntryRow::builder()
        .title("Endpoint URL")
        .text(&llm.endpoint)
        .build();
    endpoint_row.set_visible(llm.provider != ProviderKind::Local);
    provider_group.add(&endpoint_row);

    let local_group = adw::PreferencesGroup::builder()
        .title("Local Inference")
        .description("Configure onboard GGUF models.")
        .build();

    let override_model_switch = gtk::Switch::builder()
        .valign(gtk::Align::Center)
        .active(llm.override_model_path)
        .build();
    let override_model_row = adw::ActionRow::builder()
        .title("Custom Model Path")
        .subtitle("Use a specific .gguf file instead of defaults")
        .build();
    override_model_row.add_suffix(&override_model_switch);
    override_model_row.set_activatable_widget(Some(&override_model_switch));
    local_group.add(&override_model_row);

    let llm_model_row = adw::EntryRow::builder()
        .title("File Path")
        .text(&llm.local_model_path)
        .build();
    // Only show direct file path entry if override is enabled
    // Note: binding visibility is done in window.rs usually, but we set initial state here
    llm_model_row.set_sensitive(llm.override_model_path);
    local_group.add(&llm_model_row);

    // Hardware Acceleration
    let device_group = adw::PreferencesGroup::builder()
        .title("Hardware")
        .build();

    let gpu_names: Vec<String> = std::iter::once("CPU Only".to_string())
        .chain(gpus.iter().map(|g| g.name.clone()))
        .collect();
    let gpu_strings: Vec<&str> = gpu_names.iter().map(|s| s.as_str()).collect();
    let gpu_list = gtk::StringList::new(gpu_strings.as_slice());

    let gpu_combo = adw::ComboRow::builder()
        .title("Accelerator")
        .model(&gpu_list)
        .build();

    let selected_idx = if llm.force_cpu_only {
        0
    } else if let Some(ref device) = llm.preferred_device {
        gpus.iter()
            .position(|g| &g.id == device)
            .map(|i| i + 1) // +1 for CPU offset
            .unwrap_or(0)
    } else {
        0
    };
    gpu_combo.set_selected(selected_idx as u32);
    device_group.add(&gpu_combo);

    let gpu_model_row = adw::EntryRow::builder()
        .title("GPU Model")
        .text(&llm.default_gpu_model)
        .build();
    let gpu_download_button = gtk::Button::builder()
        .icon_name("folder-download-symbolic")
        .valign(gtk::Align::Center)
        .tooltip_text("Download default model")
        .css_classes(["flat"]) 
        .build();
    gpu_model_row.add_suffix(&gpu_download_button);
    device_group.add(&gpu_model_row);

    let cpu_model_row = adw::EntryRow::builder()
        .title("CPU Model")
        .text(&llm.default_cpu_model)
        .build();
    let cpu_download_button = gtk::Button::builder()
        .icon_name("folder-download-symbolic")
        .valign(gtk::Align::Center)
        .tooltip_text("Download default model")
        .css_classes(["flat"])
        .build();
    cpu_model_row.add_suffix(&cpu_download_button);
    device_group.add(&cpu_model_row);
    
    let reset_defaults_button = gtk::Button::builder()
        .label("Reset to Defaults")
        .margin_top(12)
        .margin_bottom(12)
        .css_classes(["flat"])
        .build();
    local_group.add(&reset_defaults_button);

    local_group.add(&device_group);

    let advanced_group = adw::PreferencesGroup::builder()
        .title("Generation")
        .build();
    
    let max_tokens_row = adw::ActionRow::builder()
        .title("Max Tokens")
        .build();
    let max_tokens_spin = gtk::SpinButton::builder()
        .adjustment(&gtk::Adjustment::new(
            llm.max_completion_tokens as f64,
            1.0, 512.0, 1.0, 10.0, 0.0,
        ))
        .valign(gtk::Align::Center)
        .build();
    max_tokens_row.add_suffix(&max_tokens_spin);
    advanced_group.add(&max_tokens_row);
    
    // Credentials
    let secrets_group = adw::PreferencesGroup::builder()
        .title("Security")
        .build();
    let token_row = adw::PasswordEntryRow::builder()
        .title("API Key")
        .build();
    secrets_group.add(&token_row);

    page.add(&provider_group);
    page.add(&local_group);
    page.add(&advanced_group);
    page.add(&secrets_group);

    (
        page,
        provider_row,
        endpoint_row,
        override_model_switch,
        llm_model_row,
        gpu_combo,
        gpu_model_row,
        gpu_download_button,
        cpu_model_row,
        cpu_download_button,
        reset_defaults_button,
        max_tokens_spin,
    )
}

const PROVIDERS: &[(ProviderKind, &str)] = &[
    (ProviderKind::OpenAI, "OpenAI"),
    (ProviderKind::Gemini, "Gemini"),
    (ProviderKind::Local, "Local (llama.cpp)"),
];

pub(super) fn provider_index(kind: &ProviderKind) -> usize {
    PROVIDERS.iter().position(|(k, _)| k == kind).unwrap_or(0)
}

pub(super) fn provider_from_index(idx: u32) -> ProviderKind {
    PROVIDERS
        .get(idx as usize)
        .map(|(kind, _)| *kind)
        .unwrap_or(ProviderKind::OpenAI)
}

fn build_theming_page() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Appearance")
        .icon_name("preferences-desktop-theme-symbolic")
        .build();
    let group = adw::PreferencesGroup::builder()
        .title("Style")
        .build();
    let theme_switch = gtk::Switch::builder().valign(gtk::Align::Center).build();
    let theme_row = adw::ActionRow::builder()
        .title("System Code Scheme")
        .subtitle("Inherit light/dark preference")
        .build();
    theme_row.add_suffix(&theme_switch);
    theme_row.set_activatable_widget(Some(&theme_switch));
    group.add(&theme_row);

    page.add(&group);
    page
}
