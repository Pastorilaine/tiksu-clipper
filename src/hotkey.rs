use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use parking_lot::Mutex;
use std::thread;

/// Any key combo global-hotkey understands: "F10", "Ctrl+Shift+F10", "Alt+Q", "Ctrl+1", ...
pub fn parse_hotkey(input: &str) -> Option<HotKey> {
    input.trim().parse().ok()
}

/// Hotkey text from a key press captured in the UI. A plain letter or digit would steal typing
/// in every program, so those need a modifier; F-keys may stand alone.
pub fn format_hotkey(ctrl: bool, alt: bool, shift: bool, key: &str) -> Result<String, String> {
    let is_f_key = key.strip_prefix('F').is_some_and(|n| n.parse::<u8>().is_ok());
    if !(ctrl || alt || shift || is_f_key) {
        return Err("Tavallinen näppäin tarvitsee Ctrl-, Alt- tai Shift-yhdistelmän.".to_string());
    }
    let text = [ctrl.then_some("Ctrl"), alt.then_some("Alt"), shift.then_some("Shift")]
        .into_iter()
        .flatten()
        .chain([key])
        .collect::<Vec<_>>()
        .join("+");
    parse_hotkey(&text).map(|_| text).ok_or_else(|| format!("Näppäintä \"{}\" ei voi käyttää pikanäppäimenä.", key))
}

/// The global clip hotkeys. Must live on the UI thread: Windows delivers WM_HOTKEY to the
/// thread that registered it, and that thread's message loop is eframe's.
pub struct HotkeyRegistry {
    manager: GlobalHotKeyManager,
    current: Vec<HotKey>,
    /// Hotkey id -> clip length it saves.
    secs: Arc<Mutex<HashMap<u32, u64>>>,
}

impl HotkeyRegistry {
    pub fn new(trigger_tx: Sender<u64>) -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
        let secs = Arc::new(Mutex::new(HashMap::new()));
        let map = secs.clone();
        thread::spawn(move || {
            while let Ok(event) = GlobalHotKeyEvent::receiver().recv() {
                if event.state == HotKeyState::Pressed
                    && let Some(secs) = map.lock().get(&event.id) {
                        let _ = trigger_tx.send(*secs);
                    }
            }
        });
        Ok(Self { manager, current: Vec::new(), secs })
    }

    /// Replaces all registered hotkeys with `(key, clip secs)` pairs. Returns user-facing (Finnish) errors.
    pub fn set(&mut self, keys: &[(String, u64)]) -> Vec<String> {
        for old in self.current.drain(..) {
            let _ = self.manager.unregister(old);
        }
        let mut map = self.secs.lock();
        map.clear();
        let mut errors = Vec::new();
        for (key, secs) in keys {
            let Some(hotkey) = parse_hotkey(key) else {
                errors.push(format!("Tuntematon pikanäppäin \"{}\"", key));
                continue;
            };
            if map.contains_key(&hotkey.id()) {
                errors.push(format!("Pikanäppäin {} on valittu kahdesti.", key));
                continue;
            }
            match self.manager.register(hotkey) {
                Ok(()) => {
                    map.insert(hotkey.id(), *secs);
                    self.current.push(hotkey);
                }
                Err(global_hotkey::Error::AlreadyRegistered(_)) => errors.push(format!(
                    "Pikanäppäin {} on jo toisen ohjelman käytössä (esim. NVIDIA/AMD-overlay tai toinen tallennin). Valitse toinen näppäin.",
                    key
                )),
                Err(e) => errors.push(format!("Pikanäppäimen {} rekisteröinti epäonnistui: {}", key, e)),
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_and_new_hotkeys_parse() {
        for ok in ["F10", "Alt+F8", "Ctrl+Shift+F10", "Ctrl+Q", "Alt+1", "Ctrl+Space"] {
            assert!(parse_hotkey(ok).is_some(), "{}", ok);
        }
        for bad in ["", "Ctrl+", "Ctrl+Nope", "Q+W"] {
            assert!(parse_hotkey(bad).is_none(), "{}", bad);
        }
        // Same combo, same id: the duplicate check relies on it.
        assert_eq!(parse_hotkey("ctrl+f9").map(|h| h.id()), parse_hotkey("Ctrl+F9").map(|h| h.id()));
    }

    #[test]
    fn captured_keys_are_formatted_and_checked() {
        assert_eq!(format_hotkey(false, false, false, "F9").as_deref(), Ok("F9"));
        assert_eq!(format_hotkey(true, false, true, "Q").as_deref(), Ok("Ctrl+Shift+Q"));
        assert!(format_hotkey(false, false, false, "Q").is_err());
        assert!(format_hotkey(true, false, false, "Wibble").is_err());
    }
}
