//! App settings: updates, start-up, notifications, about.

use super::super::*;

impl SillyBorgApp {
    pub(in crate::ui) fn app(&mut self, ui: &mut egui::Ui) {
        tk::page_head(ui, "Sovellus", "Päivitykset, käynnistys, taustalla toiminta ja ilmoitukset.", |_| {});
        let mut restart = false;
        tk::content(ui, true, |ui| {
            restart = self.update_panel(ui);

            tk::panel(ui, POWER, "Käynnistys", |ui| {
                tk::toggle(ui, "Käynnistä Windowsin mukana", "Aukeaa suoraan ilmaisinalueelle, puskuri päällä.", &mut self.config.autostart);
                tk::toggle(ui, "Sulje ilmaisinalueelle", "Ikkunan sulkeminen jättää puskurin käyntiin taustalle. Lopetus onnistuu ilmaisinalueen kuvakkeesta.", &mut self.config.close_to_tray);
            });

            tk::panel(ui, BELL, "Ilmoitukset", |ui| {
                tk::toggle(ui, "Ilmoitus ruudun kulmassa", "Näkyy ikkunoitujen ja reunattomien pelien päällä.", &mut self.config.overlay);
                tk::toggle(ui, "Merkkiääni", "Eri ääni onnistumiselle ja virheelle.", &mut self.config.play_sound);
                tk::row(ui, "Kokeile", "Näyttää ja soittaa ilmoituksen valituilla asetuksilla.", |ui| {
                    if tk::button(ui, Kind::Secondary, Size::Sm, "Kokeile ilmoitusta", true).clicked() {
                        if self.config.overlay {
                            crate::overlay::show("Leike tallennettu", "30 s · esimerkki.mp4", true);
                        }
                        if self.config.play_sound {
                            crate::notification::play_chime(true);
                        }
                    }
                });
            });

            tk::panel(ui, INFO, "Tietoja", |ui| {
                tk::row(ui, "Versio", "Tiksu Clipper on avointa lähdekoodia (MIT).", |ui| {
                    ui.label(tk::mono(update::CURRENT, 12.0).color(tk::TEXT));
                });
                tk::row(ui, "Näytönohjain", "", |ui| {
                    ui.add(egui::Label::new(tk::body(&self.gpu_name)).truncate());
                });
                tk::row(ui, "Loki ja asetukset", "Lokista on apua, jos jokin ei toimi.", |ui| {
                    if tk::button(ui, Kind::Ghost, Size::Sm, "Asetuskansio", true).clicked()
                        && let Some(dir) = AppConfig::config_path().parent()
                    {
                        let _ = std::process::Command::new("explorer").arg(dir).spawn();
                    }
                    if tk::button(ui, Kind::Secondary, Size::Sm, "Avaa loki", true).clicked() {
                        let _ = std::process::Command::new("notepad").arg(crate::applog::path()).spawn();
                    }
                });
            });
        });
        if restart {
            self.restart_for_update(ui.ctx());
        }
    }

    /// Returns true when the user asked to restart into the installed update.
    fn update_panel(&mut self, ui: &mut egui::Ui) -> bool {
        let state = self.updater.state();
        let busy = matches!(state, UpdateState::Checking | UpdateState::Downloading(_));
        let mut restart = false;
        tk::panel(ui, ARROW_CIRCLE_UP, "Päivitykset", |ui| {
            tk::toggle(
                ui,
                "Päivitä automaattisesti",
                "Lataa ja asentaa taustalla. Uusi versio käynnistyy seuraavalla kerralla. Vain Tiksun allekirjoittamat versiot asennetaan.",
                &mut self.config.auto_update,
            );
            tk::row_custom(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width((ui.available_width() - 240.0).max(220.0));
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.label(tk::body(format!("Nykyinen versio {}", update::CURRENT)));
                        match &state {
                            UpdateState::Idle => tk::note(ui, "Ei vielä tarkistettu tällä käynnistyksellä."),
                            UpdateState::Checking | UpdateState::Downloading(_) => {
                                ui.horizontal(|ui| {
                                    ui.add(egui::Spinner::new().color(tk::ACCENT));
                                    let text = match &state {
                                        UpdateState::Downloading(m) => format!("Ladataan versiota {}…", m.version),
                                        _ => "Tarkistetaan…".to_string(),
                                    };
                                    ui.label(tk::hint(text));
                                });
                            }
                            UpdateState::UpToDate => {
                                tk::badge(ui, Tone::Success, "Ajan tasalla");
                            }
                            UpdateState::Available(m) => {
                                tk::badge(ui, Tone::Info, &format!("Uusi versio {}", m.version));
                                if !m.notes.is_empty() {
                                    tk::note(ui, &m.notes);
                                }
                            }
                            UpdateState::Installed(m) => {
                                tk::badge(ui, Tone::Success, &format!("Versio {} asennettu", m.version));
                                if !m.notes.is_empty() {
                                    tk::note(ui, &m.notes);
                                }
                            }
                            UpdateState::Failed(e) => {
                                tk::badge(ui, Tone::Danger, "Päivitys epäonnistui");
                                tk::note(ui, e);
                            }
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if matches!(state, UpdateState::Installed(_)) {
                            restart = tk::button(ui, Kind::Primary, Size::Sm, "Käynnistä uudelleen nyt", true).clicked();
                        }
                        if matches!(state, UpdateState::Available(_)) && tk::button(ui, Kind::Primary, Size::Sm, "Lataa ja asenna", true).clicked() {
                            self.updater.check(true);
                        }
                        if tk::button(ui, Kind::Secondary, Size::Sm, "Tarkista nyt", !busy).clicked() {
                            self.updater.check(self.saved.auto_update);
                        }
                        if tk::button(ui, Kind::Ghost, Size::Sm, "Lataussivu", true).clicked() {
                            let _ = std::process::Command::new("explorer").arg(update::DOWNLOAD_PAGE).spawn();
                        }
                    });
                });
            });
        });
        restart
    }
}
