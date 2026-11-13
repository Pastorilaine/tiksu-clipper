//! Plain-text log at %APPDATA%\SillyBorg\tiksu.log. `println!`/`eprintln!` route here (see main.rs).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const MAX_BYTES: u64 = 1024 * 1024;

static FILE: OnceLock<Option<Mutex<File>>> = OnceLock::new();

pub fn path() -> PathBuf {
    crate::config::AppConfig::config_path().with_file_name("tiksu.log")
}

fn file() -> &'static Option<Mutex<File>> {
    FILE.get_or_init(|| {
        let path = path();
        // ponytail: start over past 1 MB instead of rotating; the log is for the latest problem only
        let truncate = std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES);
        let _ = std::fs::create_dir_all(path.parent()?);
        let file = OpenOptions::new()
            .create(true)
            .append(!truncate)
            .write(true)
            .truncate(truncate)
            .open(path)
            .ok()?;
        Some(Mutex::new(file))
    })
}

pub fn line(args: std::fmt::Arguments) {
    if let Some(f) = file()
        && let Ok(mut f) = f.lock() {
            let _ = writeln!(f, "{} {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), args);
        }
}

/// Panics go to the log too; a windows-subsystem app has no console to print them on.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| line(format_args!("PANIC: {}", info))));
}
