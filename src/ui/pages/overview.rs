//! Overview: state at a glance, what needs attention, the latest clips, and a three-step guide until
//! the first clip exists.

use super::super::*;

/// What fixes a problem listed on the overview.
pub(in crate::ui) enum Fix {
    Open(Page),
    RetryFfmpeg,
    /// Nothing to press: it fixes itself (e.g. a download in progress).
    Wait,
    RestartForUpdate,
}

impl SillyBorgApp {
    pub(in crate::ui) fn overview(&mut self, ui: &mut egui::Ui) {
        let (segments, buffered, bytes) = self.shared.ring.stats();
        let status = self.shared.status.lock().clone();
        let cfg = self.shared.live_config();
        let saves = self.shared.clips_saved.load(Ordering::SeqCst);
        self.library.refresh(&self.saved.output_dir, saves, false);

        let desc = format!(
            "Puskuri pitää muistissa viimeiset {}. Paina {} tallentaaksesi ne leikkeeksi.",
            fmt_secs(cfg.clip_duration_secs),
            cfg.hotkey
        );
        tk::page_head(ui, "Yleiskatsaus", &desc, |ui| {
            if tk::button(ui, Kind::Primary, Size::Md, "Tallenna leike", true).clicked() {
                let ctx = ui.ctx().clone();
                self.save_clip(&ctx);
            }
            if tk::button(ui, Kind::Secondary, Size::Md, &format!("{}  Avaa kansio", FOLDER_OPEN), true).clicked() {
                self.open_output_dir();
            }
        });

        let memory = if cfg.buffer_in_memory {
            format!("{} segmenttiä · {} RAM", segments, fmt_size(bytes as u64))
        } else {
            format!("{} segmenttiä levyllä", segments)
        };
        let (source_sub, source_color) = match (&status.active_profile, status.fallback) {
            (_, true) => ("Ikkunaa ei löydy, kuvataan näyttöä".to_string(), tk::WARN),
            (Some(exe), _) => (format!("Peliprofiili: {}", exe), tk::SUCCESS),
            (None, _) => (
                match cfg.capture_source_type {
                    CaptureTargetType::Monitor => "Koko näyttö".to_string(),
                    CaptureTargetType::Window => "Sovellusikkuna".to_string(),
                },
                tk::MUTED,
            ),
        };
        let audio = match (cfg.desktop_audio, cfg.record_microphone) {
            (DesktopAudioMode::Off, false) => ("Ei ääntä".to_string(), "Leike on mykkä".to_string()),
            (DesktopAudioMode::Off, true) => ("Mikki".to_string(), cfg.microphone_device.clone()),
            (DesktopAudioMode::Device, mic) => ("Laite".to_string(), format!("{}{}", cfg.output_device, if mic { " + mikki" } else { "" })),
            (DesktopAudioMode::Application, mic) => {
                ("Sovellus".to_string(), format!("{}{}", cfg.app_audio_process, if mic { " + mikki" } else { "" }))
            }
        };
        let (hk_sub, hk_color) = if !self.hotkey_errors.is_empty() {
            ("Varattu toiselle ohjelmalle".to_string(), tk::WARN)
        } else if !cfg.extra_hotkeys.is_empty() {
            (format!("+ {} lisänäppäintä", cfg.extra_hotkeys.len()), tk::MUTED)
        } else {
            ("Toimii myös peleissä".to_string(), tk::MUTED)
        };
        let stats = [
            tk::Stat { value: format!("{:.0} / {} s", buffered, cfg.max_clip_secs()), label: "Puskurissa", sub: memory, color: tk::MUTED },
            tk::Stat { value: status.target.clone(), label: "Kuvauslähde", sub: source_sub, color: source_color },
            tk::Stat { value: audio.0, label: "Ääni", sub: audio.1, color: tk::MUTED },
            tk::Stat { value: cfg.hotkey.clone(), label: "Pikanäppäin", sub: hk_sub, color: hk_color },
        ];

        let problems = self.problems();
        let has_clips = !self.library.clips.is_empty();
        let running = status.state == RunState::Running;
        tk::content(ui, false, |ui| {
            tk::stat_strip(ui, &stats);
            // Problems first: they explain why something doesn't work.
            if !problems.is_empty() {
                self.problems_panel(ui, &problems);
            }
            if !has_clips {
                self.guide_panel(ui, running, &cfg.hotkey, cfg.clip_duration_secs);
            }
            self.recent_clips_panel(ui);
        });
    }

    pub(in crate::ui) fn problems(&self) -> Vec<(String, Fix)> {
        let status = self.shared.status.lock().clone();
        let mut problems: Vec<(String, Fix)> = Vec::new();
        if !status.ffmpeg_ok {
            use crate::ffmpeg::Setup;
            problems.push(match crate::ffmpeg::state() {
                Setup::Downloading(pct) => (
                    format!("Ladataan leikkeiden tallennukseen tarvittavaa ffmpegiä ({} %). Se tehdään vain kerran, puskuri kuvaa jo.", pct),
                    Fix::Wait,
                ),
                Setup::Unpacking => ("Puretaan ffmpegiä…".to_string(), Fix::Wait),
                Setup::Failed(e) => (format!("ffmpegin lataus epäonnistui: {}", e), Fix::RetryFfmpeg),
                Setup::Idle => ("ffmpeg puuttuu, joten leikkeitä ei voi tallentaa.".to_string(), Fix::RetryFfmpeg),
            });
        }
        if let RunState::Failed(e) = &status.state {
            problems.push((e.clone(), Fix::Open(Page::Source)));
        }
        if status.fallback {
            problems.push((
                "Valittua ikkunaa ei löytynyt, joten kuvataan ensisijaista näyttöä. Kuvaus vaihtuu ikkunaan heti, kun se avautuu.".to_string(),
                Fix::Open(Page::Source),
            ));
        }
        problems.extend(self.hotkey_errors.iter().map(|e| (e.clone(), Fix::Open(Page::Hotkey))));
        problems.extend(status.audio_warnings.iter().map(|w| (w.clone(), Fix::Open(Page::Audio))));
        problems.extend(self.autostart_error.iter().map(|e| (e.clone(), Fix::Open(Page::App))));
        match self.updater.state() {
            UpdateState::Installed(m) => problems.push((
                format!("Tiksu Clipper {} on asennettu. Se otetaan käyttöön, kun sovellus käynnistyy uudelleen.", m.version),
                Fix::RestartForUpdate,
            )),
            UpdateState::Available(m) => problems.push((format!("Uusi versio {} on saatavilla.", m.version), Fix::Open(Page::App))),
            _ => {}
        }
        problems
    }

    /// Shown until the first clip exists: how the app is used, in three steps.
    fn guide_panel(&mut self, ui: &mut egui::Ui, running: bool, hotkey: &str, secs: u64) {
        tk::panel(ui, LIST_CHECKS, "Aloitus", |ui| {
            let steps = [
                (
                    running,
                    "Puskuri tallentaa taustalla".to_string(),
                    format!("Viimeiset {} odottavat muistissa. Pelaa tai tee mitä tahansa, mitään ei tarvitse painaa.", fmt_secs(secs)),
                ),
                (false, format!("Kun jotain tapahtuu, paina {}", hotkey), "Viimeiset sekunnit tallentuvat leikkeeksi. Toimii myös koko näytön peleissä.".to_string()),
                (false, "Leike ilmestyy Leikkeet-sivulle".to_string(), "Sieltä voit toistaa, leikata ja poistaa sen.".to_string()),
            ];
            for (i, (done, title, sub)) in steps.iter().enumerate() {
                tk::row_custom(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                        let p = ui.painter();
                        if *done {
                            p.text(rect.center(), Align2::CENTER_CENTER, CHECK_CIRCLE, egui::FontId::proportional(20.0), tk::SUCCESS);
                        } else {
                            p.circle_stroke(rect.center(), 10.0, Stroke::new(1.0, tk::LINE_STRONG));
                            p.text(rect.center(), Align2::CENTER_CENTER, (i + 1).to_string(), tk::semibold_font(11.0), tk::MUTED);
                        }
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.add(egui::Label::new(tk::body(title)).wrap());
                            tk::note(ui, sub);
                        });
                    });
                });
            }
        });
    }

    fn problems_panel(&mut self, ui: &mut egui::Ui, problems: &[(String, Fix)]) {
        let count = format!("{} kpl", problems.len());
        tk::panel_with(
            ui,
            WARNING,
            "Vaatii huomiota",
            |ui| {
                tk::badge(ui, Tone::Warning, &count);
            },
            |ui| {
                for (text, fix) in problems {
                    tk::row_custom(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.set_width((ui.available_width() - 190.0).max(200.0));
                                ui.add(egui::Label::new(tk::body(text)).wrap());
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match fix {
                                Fix::Open(page) => {
                                    if tk::button(ui, Kind::Secondary, Size::Sm, &format!("Avaa {}", page.label().to_lowercase()), true).clicked() {
                                        self.page = *page;
                                    }
                                }
                                Fix::RestartForUpdate => {
                                    if tk::button(ui, Kind::Primary, Size::Sm, "Käynnistä uudelleen", true).clicked() {
                                        let ctx = ui.ctx().clone();
                                        self.restart_for_update(&ctx);
                                    }
                                }
                                Fix::RetryFfmpeg => {
                                    if tk::button(ui, Kind::Primary, Size::Sm, "Lataa ffmpeg", true).clicked() {
                                        self.shared.ensure_ffmpeg();
                                    }
                                }
                                Fix::Wait => {
                                    ui.add(egui::Spinner::new().color(tk::ACCENT));
                                }
                            });
                        });
                    });
                }
            },
        );
    }

    fn recent_clips_panel(&mut self, ui: &mut egui::Ui) {
        let saving = self.shared.saving.load(Ordering::SeqCst);
        let last = self.shared.last_clip.lock().clone();
        let clips: Vec<ClipEntry> = self.library.clips.iter().take(5).cloned().collect();
        let mut show_all = false;
        let mut action = None;
        tk::panel_with(
            ui,
            FILM_STRIP,
            "Viimeisimmät leikkeet",
            |ui| {
                if tk::button(ui, Kind::Ghost, Size::Sm, "Näytä kaikki", true).clicked() {
                    show_all = true;
                }
            },
            |ui| {
                if saving {
                    tk::row_custom(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().color(tk::ACCENT));
                            ui.label(tk::body("Tallennetaan leikettä…"));
                        });
                    });
                } else if let Some(Err(e)) = last.as_ref().map(|c| &c.result) {
                    tk::row_custom(ui, |ui| {
                        ui.horizontal(|ui| {
                            tk::badge(ui, Tone::Danger, "Tallennus epäonnistui");
                            ui.add(egui::Label::new(tk::body(e)).wrap());
                        });
                    });
                }
                if clips.is_empty() {
                    let hotkey = self.saved.hotkey.clone();
                    tk::empty(ui, &format!("Ei vielä leikkeitä. Paina {} tai Tallenna leike.", hotkey), |_| {});
                }
                for clip in &clips {
                    if let Some(a) = self.clip_row(ui, clip) {
                        action = Some(a);
                    }
                }
            },
        );
        if show_all {
            self.page = Page::Clips;
        }
        if let Some(action) = action {
            self.clip_action(ui.ctx(), action);
        }
    }
}
