//! Audio: desktop sound, microphone, separate tracks.

use super::super::*;

impl SillyBorgApp {
    pub(in crate::ui) fn audio(&mut self, ui: &mut egui::Ui) {
        tk::page_head(ui, "Äänet", "Tietokoneen äänet ja mikrofoni tallentuvat leikkeeseen.", |_| {});
        let warnings = self.shared.status.lock().audio_warnings.clone();
        let mut refresh = false;
        tk::content(ui, true, |ui| {
            if !warnings.is_empty() {
                tk::panel(ui, WARNING, "Huomio", |ui| {
                    for w in &warnings {
                        tk::row_custom(ui, |ui| {
                            ui.add(egui::Label::new(tk::body(w)).wrap());
                        });
                    }
                });
            }

            tk::panel(ui, SPEAKER_HIGH, "Tietokoneen äänet", |ui| {
                tk::row_wide(ui, "Mitä tallennetaan", "Kaikki äänilaitteen äänet tai vain yhden sovelluksen äänet.", |ui| {
                    tk::segmented(
                        ui,
                        &mut self.config.desktop_audio,
                        &[
                            (DesktopAudioMode::Off, "Ei ääniä"),
                            (DesktopAudioMode::Device, "Äänilähde"),
                            (DesktopAudioMode::Application, "Vain sovellus"),
                        ],
                    );
                });
                match self.config.desktop_audio {
                    DesktopAudioMode::Off => {
                        tk::row_custom(ui, |ui| tk::note(ui, "Pelin ja sovellusten äänet eivät tallennu."));
                    }
                    DesktopAudioMode::Device => {
                        tk::row_wide(ui, "Äänilähde", "Tallentaa kaiken, mitä valitusta laitteesta kuuluu.", |ui| {
                            refresh |= Self::device_picker(ui, "output", &mut self.config.output_device, &self.output_devices, "");
                        });
                    }
                    DesktopAudioMode::Application => {
                        tk::row_wide(ui, "Sovellus", "Vain tämän sovelluksen äänet, esim. peli tai Discord.", |ui| {
                            refresh |= Self::device_picker(ui, "app_audio", &mut self.config.app_audio_process, &self.audio_apps, "Valitse sovellus");
                        });
                    }
                }
            });

            tk::panel(ui, MICROPHONE, "Mikrofoni", |ui| {
                tk::toggle(ui, "Tallenna mikrofoni", "Oma äänesi leikkeeseen. Pois päältä leikkeessä kuuluvat vain tietokoneen äänet.", &mut self.config.record_microphone);
                if self.config.record_microphone {
                    tk::row_wide(ui, "Mikrofonilaite", "", |ui| {
                        refresh |= Self::device_picker(ui, "mic", &mut self.config.microphone_device, &self.microphones, "");
                    });
                }
            });

            tk::panel(ui, WAVEFORM, "Ääniraidat", |ui| {
                tk::toggle(
                    ui,
                    "Erilliset raidat",
                    "Ensimmäisellä raidalla on aina kaikki äänet yhdessä. Tämä lisää tietokoneen ja mikrofonin omille raidoilleen editointia varten.",
                    &mut self.config.separate_audio_tracks,
                );
                if self.config.separate_audio_tracks && !(self.config.record_microphone && self.config.desktop_audio != DesktopAudioMode::Off) {
                    tk::row_custom(ui, |ui| {
                        ui.add(egui::Label::new(tk::hint("Erillisiä raitoja tulee vasta, kun sekä tietokoneen äänet että mikrofoni ovat päällä.").color(tk::WARN)).wrap());
                    });
                }
            });
        });
        if refresh {
            self.refresh_devices(ui.ctx());
        }
    }

    fn device_picker(ui: &mut egui::Ui, id: &str, value: &mut String, options: &[String], placeholder: &str) -> bool {
        let mut refresh = false;
        ui.horizontal(|ui| {
            let text = if value.is_empty() { placeholder.to_string() } else { value.clone() };
            egui::ComboBox::from_id_salt(id).selected_text(text).width(ui.available_width() - 110.0).show_ui(ui, |ui| {
                for o in options {
                    ui.selectable_value(value, o.clone(), o);
                }
            });
            refresh = tk::button(ui, Kind::Ghost, Size::Md, &format!("{} Päivitä", ARROWS_CLOCKWISE), true).clicked();
        });
        refresh
    }
}
