use windows_capture::monitor::Monitor;
use windows_capture::window::Window;

use crate::config::{AppConfig, CaptureTargetType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowInfo {
    pub title: String,
    pub process_name: String,
    pub width: i32,
    pub height: i32,
}

/// Enumerates all active display monitors.
pub fn enumerate_monitors() -> Vec<MonitorInfo> {
    let mut result = Vec::new();

    if let Ok(monitors) = Monitor::enumerate() {
        for (i, m) in monitors.iter().enumerate() {
            let index = i + 1;
            let name = m.name().unwrap_or_else(|_| format!("Näyttö {}", index));
            let width = m.width().unwrap_or(1920);
            let height = m.height().unwrap_or(1080);
            let is_primary = index == 1; // Primary display is index 1

            result.push(MonitorInfo {
                index,
                name,
                width,
                height,
                is_primary,
            });
        }
    }

    if result.is_empty() {
        result.push(MonitorInfo {
            index: 1,
            name: "Ensisijainen näyttö".to_string(),
            width: 1920,
            height: 1080,
            is_primary: true,
        });
    }

    result
}

/// Enumerates capturable application windows (excluding invisible, tiny, or system tool windows).
pub fn enumerate_windows() -> Vec<WindowInfo> {
    let mut result = Vec::new();

    if let Ok(windows) = Window::enumerate() {
        for w in windows {
            if !w.is_valid() {
                continue;
            }

            let title = match w.title() {
                Ok(t) if !t.trim().is_empty() => t.trim().to_string(),
                _ => continue,
            };

            // Filter out system utility windows or our own window
            if title == "Tiksu Clipper"
                || title == "Program Manager"
                || title == "Settings"
                || title == "Windows Input Experience"
            {
                continue;
            }

            let width = w.width().unwrap_or(0);
            let height = w.height().unwrap_or(0);

            // Filter out minimized or invisible windows
            if width <= 150 || height <= 150 {
                continue;
            }

            let process_name = w.process_name().unwrap_or_else(|_| "Tuntematon".to_string());

            result.push(WindowInfo {
                title,
                process_name,
                width,
                height,
            });
        }
    }

    // Sort alphabetically by title
    result.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    result
}


/// The window a config wants to capture: the biggest window of `window_process` (splash screens
/// and launchers are smaller), else a title match (configs from older versions), else the
/// foreground window when nothing is chosen. `None` for monitor capture or a missing window.
pub fn find_window(cfg: &AppConfig) -> Option<Window> {
    if cfg.capture_source_type != CaptureTargetType::Window {
        return None;
    }
    if cfg.window_process.is_empty() && cfg.window_title.is_empty() {
        return Window::foreground().ok();
    }
    let windows: Vec<Window> = Window::enumerate()
        .ok()?
        .into_iter()
        .filter(|w| w.is_valid() && w.title().is_ok_and(|t| !t.trim().is_empty()))
        .collect();
    let area = |w: &Window| w.width().unwrap_or(0) as i64 * w.height().unwrap_or(0) as i64;
    if !cfg.window_process.is_empty() {
        return windows
            .into_iter()
            .filter(|w| w.process_name().is_ok_and(|p| p.eq_ignore_ascii_case(&cfg.window_process)))
            .filter(|w| area(w) > 150 * 150)
            .max_by_key(area);
    }
    windows.into_iter().find(|w| w.title().is_ok_and(|t| t.contains(&cfg.window_title)))
}
