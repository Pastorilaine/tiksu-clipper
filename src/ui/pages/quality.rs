//! Quality: presets, clip length, frame and bit rate, where the buffer lives.

use super::super::*;

impl SillyBorgApp {
    pub(in crate::ui) fn quality(&mut self, ui: &mut egui::Ui) {
        tk::page_head(ui, "Laatu", "Valitse laatuprofiili tai säädä pituus, kuvanopeus ja bittinopeus itse.", |_| {});
        tk::content(ui, true, |ui| {
            self.presets_panel(ui);

            tk::panel(ui, TIMER, "Leikkeen pituus", |ui| {
                tk::row_wide(ui, "Pituus", "Kuinka pitkä pätkä menneisyydestä tallentuu pääpikanäppäimellä.", |ui| {
                    // A slider value between presets simply leaves no chip selected.
                    tk::segmented(ui, &mut self.config.clip_duration_secs, &CLIP_LENGTHS);
                });
                tk::row_wide(ui, "Tarkka pituus", "", |ui| {
                    ui.horizontal(|ui| {
                        let mut secs = self.config.clip_duration_secs as u32;
                        if tk::slider(ui, &mut secs, 15..=600, 5).changed() {
                            self.config.clip_duration_secs = secs as u64;
                        }
                        ui.label(tk::mono(format!("{} s", self.config.clip_duration_secs), 12.0).color(tk::TEXT));
                    });
                });
            });

            tk::panel(ui, GAUGE, "Kuva", |ui| {
                tk::row_wide(ui, "Kuvanopeus", "Nopeille peleille 120 FPS, muuten 60 riittää.", |ui| {
                    tk::segmented(ui, &mut self.config.frame_rate, &[(30, "30 FPS"), (60, "60 FPS"), (120, "120 FPS")]);
                });
                let mut mbps = self.config.bitrate / 1_000_000;
                let hint = format!("Suurempi on terävämpi mutta vie enemmän tilaa: noin {} Mt minuutissa videota.", mb_per_minute(mbps));
                tk::row_wide(ui, "Bittinopeus", &hint, |ui| {
                    ui.horizontal(|ui| {
                        if tk::slider(ui, &mut mbps, 5..=80, 1).changed() {
                            self.config.bitrate = mbps * 1_000_000;
                        }
                        ui.label(tk::mono(format!("{} Mbps", mbps), 12.0).color(tk::TEXT));
                    });
                });
            });

            tk::panel(ui, MEMORY, "Puskuri", |ui| {
                let hint = if self.config.buffer_in_memory {
                    format!(
                        "Noin {} Mt RAM:ia näillä asetuksilla. Levylle kirjoitetaan vain, kun leike tallennetaan, joten SSD ei kulu.",
                        buffer_ram_mb(&self.config)
                    )
                } else {
                    format!(
                        "Vähemmän muistia, mutta levylle kirjoitetaan jatkuvasti noin {} Mt minuutissa.",
                        mb_per_minute(self.config.bitrate / 1_000_000)
                    )
                };
                tk::row_wide(ui, "Sijainti", &hint, |ui| {
                    tk::segmented(ui, &mut self.config.buffer_in_memory, &[(true, "Muistissa"), (false, "Levyllä")]);
                });
                // More than half of the free RAM: the buffer would starve the game (or Windows).
                let free_mb = win::available_ram_bytes().map(|b| b / 1_000_000);
                if let Some(free_mb) = free_mb.filter(|free| self.config.buffer_in_memory && buffer_ram_mb(&self.config) > free / 2) {
                    tk::row_custom(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                tk::hint(format!("Muistia on vapaana vain {} Mt. Valitse lyhyempi leike, pienempi bittinopeus tai puskuri levylle.", free_mb))
                                    .color(tk::DANGER),
                            )
                            .wrap(),
                        );
                    });
                }
            });

            tk::panel(ui, CPU, "Enkooderi", |ui| {
                tk::row(ui, "Näytönohjain", "Näytönohjain koodaa videon, jolloin peli pysyy sulavana.", |ui| {
                    ui.label(tk::body(&self.gpu_name));
                });
                tk::row_custom(ui, |ui| {
                    tk::note(
                        ui,
                        "H.264 Windowsin Media Foundationin kautta. Windows valitsee laitteistoenkooderin (NVENC, AMF tai Quick Sync) automaattisesti.",
                    );
                });
            });
        });
    }

    fn presets_panel(&mut self, ui: &mut egui::Ui) {
        let current = QUALITY_PRESETS.iter().position(|p| p.2 == self.config.frame_rate && p.3 * 1_000_000 == self.config.bitrate);
        tk::panel_with(
            ui,
            SLIDERS,
            "Laatuprofiili",
            |ui| {
                if current.is_none() {
                    tk::badge(ui, Tone::Neutral, "Mukautettu");
                }
            },
            |ui| {
                tk::row_custom(ui, |ui| {
                    tk::note(ui, "Profiili asettaa kuvanopeuden ja bittinopeuden. Koko on arvio minuutista videota.");
                    // 4, 2x2 or 1 per row: never an orphan tile.
                    let n = match ui.available_width() {
                        w if w >= 4.0 * 170.0 + 36.0 => 4,
                        w if w >= 2.0 * 170.0 + 12.0 => 2,
                        _ => 1,
                    };
                    for (ri, row) in QUALITY_PRESETS.chunks(n).enumerate() {
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            ui.columns(n, |cols| {
                                for (j, (col, (name, desc, fps, mbps))) in cols.iter_mut().zip(row).enumerate() {
                                    let meta = format!("{} FPS · {} Mbps · ~{} Mt", fps, mbps, mb_per_minute(*mbps));
                                    if tk::choice_tile(col, current == Some(ri * n + j), name, &meta, desc).clicked() {
                                        self.config.frame_rate = *fps;
                                        self.config.bitrate = mbps * 1_000_000;
                                    }
                                }
                            });
                        });
                    }
                });
            },
        );
    }
}
