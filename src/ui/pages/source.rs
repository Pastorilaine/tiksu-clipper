//! Capture source: a whole monitor or one application window.

use super::super::*;

impl SillyBorgApp {
    pub(in crate::ui) fn source(&mut self, ui: &mut egui::Ui) {
        tk::page_head(ui, "Kuvauslähde", "Mitä puskuri kuvaa: koko näyttö tai yksi sovellusikkuna.", |_| {});
        let mut refresh = false;
        tk::content(ui, true, |ui| {
            tk::panel(ui, MONITOR, "Kuvattava kohde", |ui| {
                let hint = match self.config.capture_source_type {
                    CaptureTargetType::Monitor => "Kaikki näytöllä näkyvä tallentuu, myös ilmoitukset ja muut ikkunat.",
                    CaptureTargetType::Window => "Vain valittu ikkuna tallentuu, vaikka se jäisi muiden ikkunoiden taakse.",
                };
                tk::row_wide(ui, "Kohde", hint, |ui| {
                    tk::segmented(
                        ui,
                        &mut self.config.capture_source_type,
                        &[(CaptureTargetType::Monitor, "Koko näyttö"), (CaptureTargetType::Window, "Sovellusikkuna")],
                    );
                });
                match self.config.capture_source_type {
                    CaptureTargetType::Monitor => {
                        tk::row_wide(ui, "Näyttö", "Usean näytön koneessa valitse, mitä niistä kuvataan.", |ui| {
                            let label = |m: &MonitorInfo| {
                                format!("Näyttö {}: {} ({}×{}){}", m.index, m.name, m.width, m.height, if m.is_primary { " · ensisijainen" } else { "" })
                            };
                            let selected = self
                                .monitors
                                .iter()
                                .find(|m| m.index == self.config.monitor_index)
                                .map(label)
                                .unwrap_or_else(|| "Valitse näyttö".to_string());
                            egui::ComboBox::from_id_salt("monitor").selected_text(selected).width(ui.available_width()).show_ui(ui, |ui| {
                                for m in &self.monitors {
                                    ui.selectable_value(&mut self.config.monitor_index, m.index, label(m));
                                }
                            });
                        });
                    }
                    CaptureTargetType::Window => {
                        tk::row_wide(
                            ui,
                            "Ikkuna",
                            "Ikkuna löytyy sovelluksen nimellä, vaikka otsikko vaihtuisi. Jos sitä ei ole auki, kuvataan näyttöä kunnes se avautuu.",
                            |ui| {
                                ui.horizontal(|ui| {
                                    let selected = match (self.config.window_process.as_str(), self.config.window_title.as_str()) {
                                        ("", "") => "Valitse sovellus tai peli".to_string(),
                                        ("", title) => title.to_string(),
                                        (process, _) => process.to_string(),
                                    };
                                    egui::ComboBox::from_id_salt("window")
                                        .selected_text(selected)
                                        .width(ui.available_width() - 110.0)
                                        .show_ui(ui, |ui| {
                                            for w in &self.windows {
                                                let is_selected = w.process_name.eq_ignore_ascii_case(&self.config.window_process);
                                                let text = format!("{}  ·  {}", w.process_name, w.title);
                                                if ui.selectable_label(is_selected, text).clicked() {
                                                    self.config.window_process = w.process_name.clone();
                                                    self.config.window_title = w.title.clone();
                                                }
                                            }
                                        });
                                    refresh = tk::button(ui, Kind::Ghost, Size::Md, &format!("{} Päivitä", ARROWS_CLOCKWISE), true).clicked();
                                });
                                tk::note(ui, "Peli puuttuu listasta? Käynnistä se ja paina Päivitä.");
                            },
                        );
                    }
                }
            });
        });
        if refresh {
            self.refresh_devices(ui.ctx());
        }
    }
}
