#![windows_subsystem = "windows"]

/// No console in a windows-subsystem app: `println!`/`eprintln!` go to the log file.
#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => { $crate::applog::line(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! eprintln {
    ($($arg:tt)*) => { $crate::applog::line(format_args!($($arg)*)) };
}

mod applog;
mod audio;
mod buffer;
mod capture;
mod config;
mod devices;
mod hotkey;
mod library;
mod muxer;
mod notification;
mod overlay;
mod tiksu;
mod tray;
mod ui;
mod update;
mod manifest;
mod win;

use config::AppConfig;
use ui::SillyBorgApp;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    applog::install_panic_hook();
    let args: Vec<String> = std::env::args().collect();
    // After an update the new exe waits for the old one to hand over the single-instance lock.
    if let Some(pid) = args.iter().position(|a| a == "--wait-pid").and_then(|i| args.get(i + 1)?.parse().ok()) {
        win::wait_for_exit(pid, 30_000);
    }
    // A second start just brings the running instance's window up.
    let Some(instance) = win::single_instance() else {
        return Ok(());
    };
    let start_hidden = args.iter().any(|a| a == "--tray");
    let just_updated = args.iter().any(|a| a == "--updated");
    update::cleanup_old();
    println!("Tiksu Clipper {} käynnistyy{}", env!("CARGO_PKG_VERSION"), if start_hidden { " (ilmaisinalueelle)" } else { "" });

    let config = AppConfig::load();
    if config.autostart {
        // Keeps the Run entry pointing at this exe if it was moved.
        let _ = win::set_autostart(true);
    }

    // The supervisor starts capturing in the background; the window opens immediately.
    let (shared, supervisor) = capture::spawn(config.clone());
    let updater = update::Updater::new(config.auto_update);

    let native_options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 780.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("Tiksu Clipper"),
        ..Default::default()
    };

    let app_shared = shared.clone();
    let result = eframe::run_native(
        "Tiksu Clipper",
        native_options,
        Box::new(move |cc| {
            tiksu::install(&cc.egui_ctx);
            Ok(Box::new(SillyBorgApp::new(cc, config, app_shared, updater, instance, start_hidden, just_updated)))
        }),
    );
    if let Err(e) = result {
        println!("Ikkunan avaus epäonnistui: {}", e);
    }

    capture::shutdown(&shared, supervisor);
    println!("Tiksu Clipper suljettu");
    Ok(())
}
