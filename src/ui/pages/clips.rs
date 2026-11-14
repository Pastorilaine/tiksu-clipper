//! Clips: the library as a list, and the trim editor.

use super::super::*;

impl SillyBorgApp {
    pub(in crate::ui) fn clip_action(&mut self, ctx: &egui::Context, action: ClipAction) {
        match action {
            ClipAction::Play(path) => library::open(&path),
            ClipAction::Edit(path) => self.open_editor(path),
            ClipAction::Folder(path) => library::show_in_folder(&path),
            ClipAction::Copy(path) => match win::copy_file_to_clipboard(&path) {
                Ok(()) => self.notify(ctx, Tone::Success, "Leike kopioitu. Liitä se Ctrl+V:llä esimerkiksi Discordiin."),
                Err(e) => self.notify(ctx, Tone::Danger, format!("Kopiointi epäonnistui: {}", e)),
            },
            ClipAction::AskDelete(path) => self.confirm_delete = Some(path),
            ClipAction::Delete(path) => {
                self.confirm_delete = None;
                match self.library.delete(&path) {
                    Ok(()) => self.notify(ctx, Tone::Success, "Leike siirretty roskakoriin"),
                    Err(e) => self.notify(ctx, Tone::Danger, format!("Leikettä ei voitu poistaa: {}", e)),
                }
            }
        }
    }

    pub(in crate::ui) fn clips(&mut self, ui: &mut egui::Ui) {
        if self.editor.is_some() {
            return self.trim_editor(ui);
        }
        let saves = self.shared.clips_saved.load(Ordering::SeqCst);
        self.library.refresh(&self.saved.output_dir, saves, false);

        let mut rescan = false;
        tk::page_head(ui, "Leikkeet", "Tallennetut leikkeet uusimmasta alkaen. Toista, leikkaa tai poista.", |ui| {
            if tk::button(ui, Kind::Secondary, Size::Md, &format!("{}  Avaa kansio", FOLDER_OPEN), true).clicked() {
                self.open_output_dir();
            }
            rescan = tk::button(ui, Kind::Ghost, Size::Md, &format!("{} Päivitä", ARROWS_CLOCKWISE), true).clicked();
        });
        if rescan {
            self.library.refresh(&self.saved.output_dir, saves, true);
        }

        let clips = self.library.clips.clone();
        let mut action = None;
        tk::content(ui, false, |ui| {
            let count = format!("{} kpl", clips.len());
            tk::panel_with(
                ui,
                FILM_STRIP,
                "Kaikki leikkeet",
                |ui| {
                    if !clips.is_empty() {
                        tk::badge(ui, Tone::Neutral, &count);
                    }
                },
                |ui| {
                    if clips.is_empty() {
                        let text = format!(
                            "Ei vielä leikkeitä. Paina {} pelin aikana, niin leike ilmestyy tänne. Kansio: {}",
                            self.saved.hotkey,
                            self.saved.output_dir.display()
                        );
                        tk::empty(ui, &text, |_| {});
                    }
                    for clip in &clips {
                        if let Some(a) = self.clip_row(ui, clip) {
                            action = Some(a);
                        }
                    }
                },
            );
        });
        if let Some(action) = action {
            self.clip_action(ui.ctx(), action);
        }
    }

    /// One clip as a list row: thumbnail, name, facts, and the actions always visible.
    pub(in crate::ui) fn clip_row(&mut self, ui: &mut egui::Ui, clip: &ClipEntry) -> Option<ClipAction> {
        let mut action = None;
        let duration = self.library.duration(&clip.path);
        let thumb = self.library.thumb(&clip.path, duration.map_or(1.0, |d| (d / 3.0).min(5.0)), 320, 180);
        let confirming = self.confirm_delete.as_deref() == Some(clip.path.as_path());
        tk::list_item(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, resp) = ui.allocate_exact_size(vec2(96.0, 54.0), Sense::click());
                paint_frame(ui, rect, thumb.as_ref());
                ui.painter().rect_stroke(rect, CornerRadius::same(tk::R_SM), Stroke::new(1.0, tk::LINE), egui::StrokeKind::Inside);
                if resp.hovered() {
                    ui.painter().circle_filled(rect.center(), 14.0, Color32::from_black_alpha(170));
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, PLAY, egui::FontId::proportional(14.0), Color32::WHITE);
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Toista").clicked() {
                    action = Some(ClipAction::Play(clip.path.clone()));
                }

                ui.vertical(|ui| {
                    ui.set_width((ui.available_width() - 370.0).max(120.0));
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.add(egui::Label::new(tk::strong(&clip.name, 13.0)).truncate());
                    let date = chrono::DateTime::<chrono::Local>::from(clip.modified).format("%-d.%-m.%Y %H:%M");
                    let length = duration.map_or("–".to_string(), fmt_time);
                    ui.add(egui::Label::new(tk::hint(format!("{} · {} · {}", length, fmt_size(clip.size), date))).truncate());
                });

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if confirming {
                        if tk::button(ui, Kind::Danger, Size::Sm, "Vahvista poisto", true).clicked() {
                            action = Some(ClipAction::Delete(clip.path.clone()));
                        }
                    } else if tk::button(ui, Kind::Ghost, Size::Sm, TRASH, true).on_hover_text("Siirrä roskakoriin").clicked() {
                        action = Some(ClipAction::AskDelete(clip.path.clone()));
                    }
                    if tk::button(ui, Kind::Ghost, Size::Sm, FOLDER_OPEN, true).on_hover_text("Näytä kansiossa").clicked() {
                        action = Some(ClipAction::Folder(clip.path.clone()));
                    }
                    if tk::button(ui, Kind::Ghost, Size::Sm, COPY, true).on_hover_text("Kopioi leikepöydälle, liitä Ctrl+V:llä").clicked() {
                        action = Some(ClipAction::Copy(clip.path.clone()));
                    }
                    if tk::button(ui, Kind::Secondary, Size::Sm, &format!("{} Leikkaa", SCISSORS), true).clicked() {
                        action = Some(ClipAction::Edit(clip.path.clone()));
                    }
                    if tk::button(ui, Kind::Secondary, Size::Sm, &format!("{} Toista", PLAY), true).clicked() {
                        action = Some(ClipAction::Play(clip.path.clone()));
                    }
                });
            });
        });
        action
    }

    fn trim_editor(&mut self, ui: &mut egui::Ui) {
        let Some(mut ed) = self.editor.take() else { return };
        let mut close = false;
        let name = ed.path.file_name().unwrap_or_default().to_string_lossy().to_string();

        tk::page_head(ui, "Leikkaa leikettä", &name, |ui| {
            close = tk::button(ui, Kind::Secondary, Size::Md, &format!("{} Takaisin leikkeisiin", ARROW_LEFT), true).clicked();
        });

        let Some(duration) = self.library.duration(&ed.path) else {
            tk::content(ui, false, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().color(tk::ACCENT));
                    ui.label(tk::body("Luetaan leikettä…"));
                });
            });
            if !close {
                self.editor = Some(ed);
            }
            return;
        };
        let mut end = ed.end.unwrap_or(duration).min(duration);

        tk::content(ui, false, |ui| {
            tk::panel(ui, SCISSORS, "Valitse säilytettävä osa", |ui| {
                tk::row_custom(ui, |ui| {
                    // Big preview of the frame under the playhead.
                    let w = ui.available_width().min(640.0);
                    let preview = self
                        .library
                        .thumb(&ed.path, ed.preview, 640, 360)
                        .or_else(|| self.library.nearest_thumb(&ed.path, ed.preview, 640, 360))
                        .or_else(|| self.library.nearest_thumb(&ed.path, ed.preview, 160, 90));
                    let (rect, _) = ui.allocate_exact_size(vec2(w, w * 9.0 / 16.0), Sense::hover());
                    paint_frame(ui, rect, preview.as_ref());
                    ui.painter().text(rect.left_bottom() + vec2(12.0, -12.0), Align2::LEFT_BOTTOM, fmt_time(ed.preview), tk::mono_font(13.0), Color32::WHITE);
                    ui.add_space(12.0);

                    // Filmstrip with the selected range and two draggable handles.
                    let (strip, strip_resp) = ui.allocate_exact_size(vec2(w, 64.0), Sense::click());
                    const SLOTS: usize = 10;
                    let slot_w = strip.width() / SLOTS as f32;
                    for i in 0..SLOTS {
                        let t = duration * (i as f64 + 0.5) / SLOTS as f64;
                        let slot = Rect::from_min_size(pos2(strip.left() + i as f32 * slot_w, strip.top()), vec2(slot_w, strip.height()));
                        let tex = self.library.thumb(&ed.path, t, 160, 90);
                        paint_frame(ui, slot.shrink(0.5), tex.as_ref());
                    }
                    let x_of = |t: f64| strip.left() + (t / duration) as f32 * strip.width();
                    let t_of = |x: f32| (((x - strip.left()) / strip.width()) as f64 * duration).clamp(0.0, duration);
                    let p = ui.painter();
                    let dim = Color32::from_black_alpha(170);
                    p.rect_filled(Rect::from_min_max(strip.min, pos2(x_of(ed.start), strip.max.y)), 0.0, dim);
                    p.rect_filled(Rect::from_min_max(pos2(x_of(end), strip.min.y), strip.max), 0.0, dim);
                    p.rect_stroke(
                        Rect::from_min_max(pos2(x_of(ed.start), strip.min.y), pos2(x_of(end), strip.max.y)),
                        CornerRadius::same(4),
                        Stroke::new(2.0, tk::ACCENT),
                        egui::StrokeKind::Inside,
                    );
                    if strip_resp.clicked()
                        && let Some(pos) = strip_resp.interact_pointer_pos()
                    {
                        ed.preview = t_of(pos.x);
                    }
                    let min_len = 0.5f64.min(duration);
                    for (which, t) in [(0, ed.start), (1, end)] {
                        let handle = Rect::from_center_size(pos2(x_of(t), strip.center().y), vec2(10.0, strip.height() + 8.0));
                        let resp = ui.interact(handle.expand2(vec2(6.0, 0.0)), ui.id().with(("trim_handle", which)), Sense::drag());
                        let color = if resp.hovered() || resp.dragged() { Color32::WHITE } else { tk::ACCENT };
                        ui.painter().rect_filled(handle, CornerRadius::same(3), color);
                        if resp.dragged()
                            && let Some(pos) = resp.interact_pointer_pos()
                        {
                            let t = t_of(pos.x);
                            if which == 0 {
                                ed.start = t.min(end - min_len).max(0.0);
                                ed.preview = ed.start;
                            } else {
                                end = t.max(ed.start + min_len).min(duration);
                                ed.preview = end;
                            }
                        }
                        resp.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                    }
                    let px = x_of(ed.preview);
                    ui.painter().line_segment([pos2(px, strip.top() - 4.0), pos2(px, strip.bottom() + 4.0)], Stroke::new(2.0, Color32::WHITE));
                    ui.add_space(8.0);
                    tk::note(ui, "Vedä kahvoja tai klikkaa filmiliuskaa. Kirkas osa säilyy, himmeä leikkautuu pois.");
                });

                tk::row_custom(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(tk::hint("Alku"));
                        let max_start = (end - 0.5f64.min(duration)).max(0.0);
                        if ui
                            .add(egui::DragValue::new(&mut ed.start).speed(0.05).range(0.0..=max_start).suffix(" s").fixed_decimals(1))
                            .changed()
                        {
                            ed.preview = ed.start;
                        }
                        ui.add_space(12.0);
                        ui.label(tk::hint("Loppu"));
                        if ui
                            .add(egui::DragValue::new(&mut end).speed(0.05).range((ed.start + 0.5f64.min(duration))..=duration).suffix(" s").fixed_decimals(1))
                            .changed()
                        {
                            ed.preview = end;
                        }
                        ui.add_space(12.0);
                        ui.label(tk::mono(format!("Pituus {:.1} s / {:.1} s", end - ed.start, duration), 12.0).color(tk::TEXT));
                    });
                });

                tk::row_custom(ui, |ui| {
                    ui.horizontal(|ui| {
                        let busy = self.library.trimming;
                        if tk::button(ui, Kind::Primary, Size::Md, &format!("{} Tallenna leikattu kopio", SCISSORS), !busy).clicked() {
                            self.library.trim(&ed.path, ed.start, end);
                        }
                        if tk::button(ui, Kind::Secondary, Size::Md, &format!("{} Toista alkuperäinen", PLAY), true).clicked() {
                            library::open(&ed.path);
                        }
                        if busy {
                            ui.add(egui::Spinner::new().color(tk::ACCENT));
                            ui.label(tk::body("Leikataan…"));
                        }
                    });
                    tk::note(ui, "Alkuperäinen leike säilyy. Leikattu kopio tallentuu samaan kansioon nimellä …_trim.mp4.");
                });
            });
        });

        ed.end = Some(end);
        if !close {
            self.editor = Some(ed);
        }
    }
}
