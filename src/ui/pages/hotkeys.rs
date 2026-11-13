//! Hotkeys and the save folder.

use super::super::*;

impl SillyBorgApp {
    pub(in crate::ui) fn hotkey(&mut self, ui: &mut egui::Ui) {
        tk::page_head(ui, "Pikanäppäimet", "Millä näppäimillä leike tallennetaan ja minne.", |_| {});
        tk::content(ui, true, |ui| {
            tk::panel(ui, KEYBOARD, "Pääpikanäppäin", |ui| {
                let hint = format!("Tallentaa viimeiset {}. Toimii myös koko näytön peleissä.", fmt_secs(self.config.clip_duration_secs));
                tk::row(ui, "Pikanäppäin", &hint, |ui| {
                    Self::hotkey_button(ui, egui::Id::new("hotkey"), &mut self.config.hotkey, &mut self.capturing_hotkey, &mut self.capture_error);
                });
                for err in &self.hotkey_errors {
                    tk::row_custom(ui, |ui| {
                        ui.add(egui::Label::new(tk::hint(err).color(tk::DANGER)).wrap());
                    });
                }
            });

            tk::panel(ui, PLUS_CIRCLE, "Lisäpikanäppäimet", |ui| {
                tk::row_custom(ui, |ui| tk::note(ui, "Eri näppäin eri pituiselle leikkeelle, esim. nopea 15 s."));
                let mut remove = None;
                for (i, extra) in self.config.extra_hotkeys.iter_mut().enumerate() {
                    tk::row_custom(ui, |ui| {
                        ui.horizontal(|ui| {
                            Self::hotkey_button(ui, egui::Id::new(("extra_hotkey", i)), &mut extra.hotkey, &mut self.capturing_hotkey, &mut self.capture_error);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if tk::button(ui, Kind::Ghost, Size::Sm, TRASH, true).on_hover_text("Poista").clicked() {
                                    remove = Some(i);
                                }
                            });
                        });
                        tk::segmented(ui, &mut extra.secs, &EXTRA_LENGTHS);
                    });
                }
                if let Some(i) = remove {
                    self.config.extra_hotkeys.remove(i);
                }
                if self.config.extra_hotkeys.len() < MAX_EXTRA_HOTKEYS {
                    tk::row_custom(ui, |ui| {
                        if tk::button(ui, Kind::Secondary, Size::Sm, &format!("{} Lisää pikanäppäin", PLUS), true).clicked() {
                            let used: Vec<String> = self.config.all_hotkeys().into_iter().map(|(k, _)| k).collect();
                            let key = HOTKEYS.iter().find(|k| !used.iter().any(|u| u == *k)).unwrap_or(&"F9");
                            self.config.extra_hotkeys.push(ExtraHotkey { hotkey: key.to_string(), secs: 30 });
                        }
                    });
                }
            });

            if self.capturing_hotkey.is_some() {
                tk::panel(ui, INFO, "Näppäimen valinta", |ui| {
                    tk::row_custom(ui, |ui| match &self.capture_error {
                        Some(e) => {
                            ui.add(egui::Label::new(tk::body(e).color(tk::DANGER)).wrap());
                        }
                        None => {
                            ui.add(egui::Label::new(tk::body("Paina haluamaasi näppäintä tai yhdistelmää, esim. Ctrl+F10. Esc peruu.")).wrap());
                        }
                    });
                });
            }

            tk::panel(ui, FOLDER_OPEN, "Tallennuskansio", |ui| {
                tk::row_wide(ui, "Kansio", "Valmiit leikkeet tallentuvat tänne.", |ui| {
                    egui::Frame::new()
                        .fill(tk::SURFACE_2)
                        .stroke(Stroke::new(1.0, tk::LINE))
                        .corner_radius(CornerRadius::same(tk::R_MD))
                        .inner_margin(Margin::symmetric(10, 7))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.add(egui::Label::new(tk::mono(self.config.output_dir.to_string_lossy(), 12.0).color(tk::TEXT)).truncate());
                        });
                    ui.horizontal(|ui| {
                        if tk::button(ui, Kind::Secondary, Size::Md, "Selaa…", true).clicked()
                            && let Some(folder) = rfd::FileDialog::new().set_directory(&self.config.output_dir).pick_folder()
                        {
                            self.config.output_dir = folder;
                        }
                        if tk::button(ui, Kind::Ghost, Size::Md, &format!("{} Avaa kansio", FOLDER_OPEN), true).clicked() {
                            self.open_output_dir();
                        }
                    });
                });
            });
        });
    }

    /// A button showing the key; clicking it listens for the next key press (Esc cancels).
    /// The instructions or the refusal reason appear in the "Näppäimen valinta" panel.
    fn hotkey_button(ui: &mut egui::Ui, id: egui::Id, value: &mut String, capturing: &mut Option<egui::Id>, error: &mut Option<String>) {
        let listening = *capturing == Some(id);
        let label = if listening { "Paina näppäintä…".to_string() } else { format!("{}  {}", KEYBOARD, value) };
        if tk::button(ui, if listening { Kind::Primary } else { Kind::Secondary }, Size::Md, &label, true).clicked() {
            *capturing = if listening { None } else { Some(id) };
            *error = None;
        }
        if !listening {
            return;
        }
        for event in ui.input(|i| i.events.clone()) {
            if let egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } = event {
                if key == egui::Key::Escape {
                    *capturing = None;
                } else {
                    match crate::hotkey::format_hotkey(modifiers.ctrl, modifiers.alt, modifiers.shift, key.name()) {
                        Ok(text) => {
                            *value = text;
                            *capturing = None;
                            *error = None;
                        }
                        Err(e) => *error = Some(e),
                    }
                }
            }
        }
    }
}
