mod app;
mod document;
mod llm;
mod paths;
mod settings;
mod state_store;

use gtk4::{gio, glib, prelude::*};
use libadwaita as adw;

fn main() -> glib::ExitCode {
    env_logger::init();

    let app = adw::Application::builder()
        .application_id("com.wispnote.Wispnote")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    app.connect_activate(|application| {
        if let Err(err) = app::build_ui(application) {
            log::error!("Failed to start UI: {err:?}");
        }
    });

    app.run()
}
