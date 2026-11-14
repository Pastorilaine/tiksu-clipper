//! Quality: presets, clip length, picture size, frame and bit rate, where the buffer lives, the encoder.

use super::super::*;

impl SillyBorgApp {
    pub(in crate::ui) fn quality(&mut self, ui: &mut egui::Ui) {
        tk::page_head(ui, "Laatu", "Valitse laatuprofiili tai säädä pituus, kuvakoko, kuvanopeus ja bittinopeus itse.", |_| {});
        let (cap_w, cap_h) = self.capture_size();
        tk::content(ui, true, |ui| {
            self.presets_panel(ui, (cap_w, cap_h));

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

            let (enc_w, enc_h) = self.config.encode_size(cap_w, cap_h);
            let bitrate = self.config.effective_bitrate(cap_w, cap_h);
            tk::panel(ui, GAUGE, "Kuva", |ui| {
                let size_hint = if (enc_w, enc_h) == (cap_w / 2 * 2, cap_h / 2 * 2) {
                    format!("Kuva tallentuu alkuperäisessä koossa ({}×{}). Pienempi kuva keventää konetta ja pienentää tiedostoja.", enc_w, enc_h)
                } else {
                    format!("Kuva pienennetään: {}×{} tallentuu kokoon {}×{}. Kevyempi koneelle ja pienempi tiedosto.", cap_w, cap_h, enc_w, enc_h)
                };
                tk::row_wide(ui, "Kuvakoko", &size_hint, |ui| {
                    tk::segmented(ui, &mut self.config.max_height, &PICTURE_HEIGHTS);
                });
                tk::row_wide(ui, "Kuvanopeus", "Nopeille peleille 120 FPS, muuten 60 riittää. 30 FPS on kevyin.", |ui| {
                    tk::segmented(ui, &mut self.config.frame_rate, &[(30, "30 FPS"), (60, "60 FPS"), (120, "120 FPS")]);
                });
                let mut mbps = self.config.bitrate / 1_000_000;
                let hint = if self.config.scale_bitrate {
                    format!(
                        "Arvo on 1080p-kuvalle. Tällä kuvalla ({}×{}) bittinopeus on noin {} Mbps, eli {} Mt minuutissa.",
                        enc_w,
                        enc_h,
                        bitrate / 1_000_000,
                        mb_per_minute(bitrate / 1_000_000)
                    )
                } else {
                    format!("Suurempi on terävämpi mutta vie enemmän tilaa: noin {} Mt minuutissa videota.", mb_per_minute(mbps))
                };
                tk::row_wide(ui, "Bittinopeus", &hint, |ui| {
                    ui.horizontal(|ui| {
                        if tk::slider(ui, &mut mbps, 3..=150, 1).changed() {
                            self.config.bitrate = mbps * 1_000_000;
                        }
                        ui.label(tk::mono(format!("{} Mbps", mbps), 12.0).color(tk::TEXT));
                    });
                });
                tk::toggle(
                    ui,
                    "Skaalaa bittinopeus kuvakoon mukaan",
                    "Suositus. Iso tai ultralaaja näyttö saa automaattisesti enemmän bittejä, pieni kuva vähemmän. Pois päältä käytetään yllä olevaa arvoa sellaisenaan.",
                    &mut self.config.scale_bitrate,
                );
            });

            tk::panel(ui, MEMORY, "Puskuri", |ui| {
                let ram = buffer_ram_mb(&self.config, bitrate);
                let hint = if self.config.buffer_in_memory {
                    format!("Noin {} Mt RAM:ia näillä asetuksilla. Levylle kirjoitetaan vain, kun leike tallennetaan, joten SSD ei kulu.", ram)
                } else {
                    format!("Vähemmän muistia, mutta levylle kirjoitetaan jatkuvasti noin {} Mt minuutissa.", mb_per_minute(bitrate / 1_000_000))
                };
                tk::row_wide(ui, "Sijainti", &hint, |ui| {
                    tk::segmented(ui, &mut self.config.buffer_in_memory, &[(true, "Muistissa"), (false, "Levyllä")]);
                });
                // More than half of the free RAM: the buffer would starve the game (or Windows).
                let free_mb = win::available_ram_bytes().map(|b| b / 1_000_000);
                if let Some(free_mb) = free_mb.filter(|free| self.config.buffer_in_memory && ram > free / 2) {
                    tk::row_custom(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                tk::hint(format!("Muistia on vapaana vain {} Mt. Valitse lyhyempi leike, pienempi kuvakoko tai bittinopeus tai puskuri levylle.", free_mb))
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
                match &self.hw_encoder {
                    Some(name) => {
                        tk::row(ui, "Laitteistoenkoodaus", &format!("H.264, High-profiili. Käytössä: {}.", name), |ui| {
                            tk::badge(ui, Tone::Success, "Käytössä");
                        });
                    }
                    None => {
                        tk::row(
                            ui,
                            "Laitteistoenkoodaus",
                            "Näytönohjain ei tarjoa H.264-enkooderia, joten video koodataan prosessorilla. Se kuormittaa konetta: valitse Heikko kone -profiili tai pienempi kuvakoko ja 30 FPS.",
                            |ui| {
                                tk::badge(ui, Tone::Warning, "Ei löytynyt");
                            },
                        );
                    }
                }
            });
        });
    }

    fn presets_panel(&mut self, ui: &mut egui::Ui, (cap_w, cap_h): (u32, u32)) {
        let current = QUALITY_PRESETS
            .iter()
            .position(|p| p.2 == self.config.frame_rate && p.3 * 1_000_000 == self.config.bitrate && p.4 == self.config.max_height);
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
                    tk::note(ui, "Profiili asettaa kuvakoon, kuvanopeuden ja bittinopeuden. Luvut ovat tälle näytölle ja tiedoston koko arvio minuutista videota.");
                    // 3 per row, or 1 on a narrow window.
                    let n = if ui.available_width() >= 3.0 * 190.0 + 24.0 { 3 } else { 1 };
                    for (ri, row) in QUALITY_PRESETS.chunks(n).enumerate() {
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            ui.columns(n, |cols| {
                                for (j, (col, (name, desc, fps, mbps, height))) in cols.iter_mut().zip(row).enumerate() {
                                    let preview = AppConfig { bitrate: mbps * 1_000_000, max_height: *height, scale_bitrate: true, ..self.config.clone() };
                                    let (_, h) = preview.encode_size(cap_w, cap_h);
                                    let real_mbps = preview.effective_bitrate(cap_w, cap_h) / 1_000_000;
                                    let meta = format!("{}p · {} FPS · {} Mbps · ~{} Mt", h, fps, real_mbps, mb_per_minute(real_mbps));
                                    if tk::choice_tile(col, current == Some(ri * n + j), name, &meta, desc).clicked() {
                                        self.config.frame_rate = *fps;
                                        self.config.bitrate = mbps * 1_000_000;
                                        self.config.max_height = *height;
                                        self.config.scale_bitrate = true;
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
