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

    // Global heartbeat thread to verify application is responsive
    std::thread::spawn(|| {
        let mut count = 0u64;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            count += 5;
            eprintln!("[APP HEARTBEAT] Application running for {}s", count);
        }
    });

    let app = adw::Application::builder()
        .application_id("com.ghostpad.Ghostpad")
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    app.connect_activate(|application| {
        if let Err(err) = app::build_ui(application) {
            eprintln!("Failed to start UI: {err:?}");
        }
    });

    app.run()
}
