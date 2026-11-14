//! Game profiles: per-game settings that apply automatically while the game is in front.

use super::super::*;

impl SillyBorgApp {
    pub(in crate::ui) fn profiles(&mut self, ui: &mut egui::Ui) {
        tk::page_head(ui, "Peliprofiilit", "Kun peli on käynnissä, sen omat asetukset otetaan käyttöön automaattisesti.", |_| {});
        let active = self.shared.status.lock().active_profile.clone();
        let mut refresh = false;
        let mut remove = None;
        tk::content(ui, true, |ui| {
            tk::panel(ui, GAME_CONTROLLER, "Automaattinen vaihto", |ui| {
                tk::toggle(
                    ui,
                    "Käytä peliprofiileja",
                    "Profiili vaihtuu, kun peli tulee etualalle, ja palautuu, kun peli suljetaan. Pois päältä käytetään aina perusasetuksia.",
                    &mut self.config.auto_profiles,
                );
                tk::row_wide(ui, "Lisää peli", "Lista näyttää nyt auki olevat sovellukset. Käynnistä peli ja paina Päivitä.", |ui| {
                    ui.horizontal(|ui| {
                        let available: Vec<String> =
                            self.audio_apps.iter().filter(|a| self.config.profile_for(a).is_none()).cloned().collect();
                        let mut picked = None;
                        egui::ComboBox::from_id_salt("add_profile")
                            .selected_text(format!("{} Valitse peli", PLUS))
                            .width(ui.available_width() - 110.0)
                            .show_ui(ui, |ui| {
                                for a in &available {
                                    if ui.selectable_label(false, a).clicked() {
                                        picked = Some(a.clone());
                                    }
                                }
                            });
                        if let Some(exe) = picked {
                            self.config.profiles.push(GameProfile {
                                exe,
                                capture_window: true,
                                frame_rate: self.config.frame_rate,
                                bitrate: self.config.bitrate,
                                clip_duration_secs: self.config.clip_duration_secs,
                            });
                        }
                        refresh = tk::button(ui, Kind::Ghost, Size::Md, &format!("{} Päivitä", ARROWS_CLOCKWISE), true).clicked();
                    });
                });
            });

            for (i, p) in self.config.profiles.iter_mut().enumerate() {
                let is_active = active.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(&p.exe));
                if profile_panel(ui, i, p, is_active) {
                    remove = Some(i);
                }
            }
            if self.config.profiles.is_empty() {
                tk::panel(ui, LIGHTBULB, "Ei profiileja vielä", |ui| {
                    tk::row_custom(ui, |ui| {
                        tk::note(ui, "Lisää peli yllä olevasta listasta, niin sille voi valita oman laadun ja leikkeen pituuden.");
                    });
                });
            }
        });
        if refresh {
            self.refresh_devices(ui.ctx());
        }
        if let Some(i) = remove {
            self.config.profiles.remove(i);
        }
    }
}

/// One game profile's settings. Returns true when the user removes it.
fn profile_panel(ui: &mut egui::Ui, index: usize, p: &mut GameProfile, active: bool) -> bool {
    let mut remove = false;
    let exe = p.exe.clone();
    tk::panel_with(
        ui,
        GAME_CONTROLLER,
        &exe,
        |ui| {
            if active {
                tk::badge(ui, Tone::Success, "Käytössä nyt");
            }
        },
        |ui| {
            tk::toggle(ui, "Kuvaa vain pelin ikkuna", "Pois päältä käytetään perusasetusten kuvauslähdettä.", &mut p.capture_window);
            tk::row_wide(ui, "Laatu", "", |ui| {
                let mut quality = (p.frame_rate, p.bitrate / 1_000_000);
                let options: Vec<((u32, u32), &str)> = QUALITY_PRESETS.iter().map(|(name, _, fps, mbps, _)| ((*fps, *mbps), *name)).collect();
                if tk::segmented(ui, &mut quality, &options) {
                    p.frame_rate = quality.0;
                    p.bitrate = quality.1 * 1_000_000;
                }
            });
            tk::row_wide(ui, "Leikkeen pituus", "", |ui| {
                tk::segmented(ui, &mut p.clip_duration_secs, &CLIP_LENGTHS);
            });
            tk::row_custom(ui, |ui| {
                ui.push_id(index, |ui| {
                    remove = tk::button(ui, Kind::Danger, Size::Sm, &format!("{} Poista profiili", TRASH), true).clicked();
                });
            });
        },
    );
    remove
}
