use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use parking_lot::Mutex;
use std::thread;

pub fn parse_hotkey(input: &str) -> Option<HotKey> {
    let parts: Vec<&str> = input.split('+').map(|s| s.trim()).collect();
    let mut modifiers = Modifiers::empty();
    let mut code = None;

    for part in parts {
        match part.to_uppercase().as_str() {
            "CTRL" | "CONTROL" => modifiers |= Modifiers::CONTROL,
            "ALT" => modifiers |= Modifiers::ALT,
            "SHIFT" => modifiers |= Modifiers::SHIFT,
            "SUPER" | "WIN" | "CMD" => modifiers |= Modifiers::SUPER,
            "F1" => code = Some(Code::F1),
            "F2" => code = Some(Code::F2),
            "F3" => code = Some(Code::F3),
            "F4" => code = Some(Code::F4),
            "F5" => code = Some(Code::F5),
            "F6" => code = Some(Code::F6),
            "F7" => code = Some(Code::F7),
            "F8" => code = Some(Code::F8),
            "F9" => code = Some(Code::F9),
            "F10" => code = Some(Code::F10),
            "F11" => code = Some(Code::F11),
            "F12" => code = Some(Code::F12),
            _ => {}
        }
    }

    code.map(|c| {
        if modifiers.is_empty() {
            HotKey::new(None, c)
        } else {
            HotKey::new(Some(modifiers), c)
        }
    })
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
                if event.state == HotKeyState::Pressed {
                    if let Some(secs) = map.lock().get(&event.id) {
                        let _ = trigger_tx.send(*secs);
                    }
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
