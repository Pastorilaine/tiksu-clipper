use std::sync::Arc;

use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrayAction {
    Show,
    SaveClip,
    OpenFolder,
    Quit,
}

pub struct TrayManager {
    _tray_icon: TrayIcon,
    save_item: MenuItem,
}

/// 32x32 RGBA icon: mint ring with a dot, the Tiksu record mark.
fn create_tray_icon() -> Icon {
    let size = 32u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 - 15.5).powi(2) + (y as f32 - 15.5).powi(2)).sqrt();
            // Anti-aliased coverage of the ring (r 11..14) and the dot (r < 6).
            let cover = |inner: f32, outer: f32| ((outer - d).min(d - inner) + 0.5).clamp(0.0, 1.0);
            let a = cover(10.5, 14.5).max(cover(-1.0, 6.0));
            rgba.extend_from_slice(&[0x3a, 0xd9, 0xb8, (a * 255.0) as u8]);
        }
    }
    Icon::from_rgba(rgba, size, size).expect("tray icon")
}

impl TrayManager {
    /// `on_action` runs on the tray's event thread.
    pub fn new(hotkey: &str, on_action: impl Fn(TrayAction) + Send + Sync + 'static) -> Result<Self, String> {
        let menu = Menu::new();
        let show_item = MenuItem::new("Avaa Tiksu Clipper", true, None);
        let save_item = MenuItem::new(format!("Tallenna leike ({})", hotkey), true, None);
        let folder_item = MenuItem::new("Avaa leikekansio", true, None);
        let quit_item = MenuItem::new("Lopeta", true, None);
        menu.append_items(&[
            &show_item,
            &PredefinedMenuItem::separator(),
            &save_item,
            &folder_item,
            &PredefinedMenuItem::separator(),
            &quit_item,
        ])
        .map_err(|e| e.to_string())?;

        let ids = [
            (show_item.id().clone(), TrayAction::Show),
            (save_item.id().clone(), TrayAction::SaveClip),
            (folder_item.id().clone(), TrayAction::OpenFolder),
            (quit_item.id().clone(), TrayAction::Quit),
        ];
        let on_action = Arc::new(on_action);
        {
            let on_action = on_action.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                if let Some((_, action)) = ids.iter().find(|(id, _)| *id == event.id) {
                    on_action(*action);
                }
            }));
        }
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                on_action(TrayAction::Show);
            }
        }));

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Tiksu Clipper")
            .with_icon(create_tray_icon())
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self { _tray_icon: tray_icon, save_item })
    }

    pub fn set_hotkey(&self, hotkey: &str) {
        self.save_item.set_text(format!("Tallenna leike ({})", hotkey));
    }
}
