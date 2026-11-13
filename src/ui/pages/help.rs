//! Help: how the app works, the current hotkeys and answers to common questions.
//! The questions are data (`FAQ`): add a line there to add an answer.

use super::super::*;

/// (question, answer)
const FAQ: &[(&str, &str)] = &[
    (
        "Leike ei tallennu",
        "Katso Yleiskatsauksen kohta Vaatii huomiota. Yleisimmät syyt: ffmpeg latautuu vielä (vain ensimmäisellä kerralla), puskuri on pysäytetty tai pikanäppäin on toisen ohjelman, esimerkiksi NVIDIA-overlayn, käytössä.",
    ),
    (
        "Kuvassa on pelkkää mustaa tai peli ei näy",
        "Valitse Kuvauslähde-sivulta koko näyttö ja pelaa reunattomassa ikkunatilassa. Joissakin peleissä vain koko näytön kuvaus toimii.",
    ),
    (
        "Leikkeestä puuttuu ääni",
        "Valitse Äänet-sivulta Äänilähde tai Vain sovellus. Jos haluat oman äänesi mukaan, kytke myös mikrofoni päälle.",
    ),
    (
        "Paljonko muistia ja levytilaa tämä vie?",
        "Puskuri vie RAM:ia laatuasetusten mukaan, ja Laatu-sivu näyttää arvion. Levylle kirjoitetaan vasta, kun leike tallennetaan.",
    ),
    (
        "Miten leikkaan leikkeen?",
        "Valitse Leikkeet-sivulta Leikkaa, vedä kahvoja ja tallenna kopio. Alkuperäinen leike säilyy.",
    ),
    (
        "Sulkeeko ikkunan sulkeminen ohjelman?",
        "Ei, puskuri jää toimimaan taustalle ilmaisinalueelle. Lopeta ohjelma ilmaisinalueen kuvakkeen valikosta. Tämän voi muuttaa Sovellus-sivulta.",
    ),
    (
        "Miten ohjelma päivittyy?",
        "Taustalla itsestään. Sovellus-sivulta voi tarkistaa päivitykset heti. Uusi versio otetaan käyttöön seuraavalla käynnistyksellä tai Käynnistä uudelleen -napista.",
    ),
];

impl SillyBorgApp {
    pub(in crate::ui) fn help(&mut self, ui: &mut egui::Ui) {
        tk::page_head(ui, "Ohje", "Näin Tiksu Clipper toimii ja mitä tehdä, jos jokin ei toimi.", |_| {});
        let cfg = self.saved.clone();
        tk::content(ui, true, |ui| {
            tk::panel(ui, BOOK_OPEN, "Näin se toimii", |ui| {
                let steps = [
                    ("Puskuri tallentaa taustalla", "Ohjelma pitää viimeiset minuutit pakattuna muistissa. Levylle ei kirjoiteta mitään ennen tallennusta."),
                    ("Paina pikanäppäintä", "Kun jotain hienoa tapahtuu, paina pikanäppäintä. Viimeiset sekunnit tallentuvat MP4-leikkeeksi."),
                    ("Toista, leikkaa tai jaa", "Leikkeet-sivulta löydät kaikki leikkeet. Tiedosto on tavallinen MP4, jonka voi jakaa mihin tahansa."),
                ];
                for (i, (title, text)) in steps.iter().enumerate() {
                    tk::row_custom(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                            ui.painter().circle_stroke(rect.center(), 10.0, Stroke::new(1.0, tk::LINE_STRONG));
                            ui.painter().text(rect.center(), Align2::CENTER_CENTER, (i + 1).to_string(), tk::semibold_font(11.0), tk::MUTED);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.label(tk::body(*title));
                                tk::note(ui, *text);
                            });
                        });
                    });
                }
            });

            tk::panel_with(
                ui,
                KEYBOARD,
                "Pikanäppäimesi",
                |ui| {
                    if tk::button(ui, Kind::Ghost, Size::Sm, "Muuta", true).clicked() {
                        self.page = Page::Hotkey;
                    }
                },
                |ui| {
                    tk::row(ui, "Tallenna leike", &format!("Viimeiset {}.", fmt_secs(cfg.clip_duration_secs)), |ui| {
                        tk::keycap(ui, &cfg.hotkey);
                    });
                    for extra in &cfg.extra_hotkeys {
                        tk::row(ui, "Lisäpikanäppäin", &format!("Viimeiset {}.", fmt_secs(extra.secs)), |ui| {
                            tk::keycap(ui, &extra.hotkey);
                        });
                    }
                },
            );

            tk::panel(ui, QUESTION, "Usein kysyttyä", |ui| {
                for (i, (question, answer)) in FAQ.iter().enumerate() {
                    faq_item(ui, i, question, answer);
                }
            });

            tk::panel(ui, LIFEBUOY, "Ei vieläkään toimi?", |ui| {
                tk::row(ui, "Lokitiedosto", "Kerro lokin viimeiset rivit, kun kysyt apua.", |ui| {
                    if tk::button(ui, Kind::Secondary, Size::Sm, "Avaa loki", true).clicked() {
                        let _ = std::process::Command::new("notepad").arg(crate::applog::path()).spawn();
                    }
                });
                tk::row(ui, "Projektisivu", "Ohjeet, uusimmat versiot ja virheilmoitukset GitHubissa.", |ui| {
                    if tk::button(ui, Kind::Secondary, Size::Sm, &format!("{} Avaa GitHub", ARROW_SQUARE_OUT), true).clicked() {
                        let _ = std::process::Command::new("explorer").arg("https://github.com/Pastorilaine/tiksu-clipper").spawn();
                    }
                });
            });
        });
    }
}

/// A question that opens to its answer when clicked.
fn faq_item(ui: &mut egui::Ui, index: usize, question: &str, answer: &str) {
    let id = ui.id().with(("faq", index));
    let open: bool = ui.data(|d| d.get_temp(id)).unwrap_or(false);
    tk::row_custom(ui, |ui| {
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::click());
        let p = ui.painter();
        let color = if resp.hovered() { tk::TEXT } else { tk::MUTED };
        p.text(rect.left_center(), Align2::LEFT_CENTER, if open { CARET_DOWN } else { CARET_RIGHT }, egui::FontId::proportional(14.0), color);
        p.text(rect.left_center() + vec2(22.0, 0.0), Align2::LEFT_CENTER, question, tk::semibold_font(13.0), tk::TEXT);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            ui.data_mut(|d| d.insert_temp(id, !open));
        }
        if open {
            ui.horizontal(|ui| {
                ui.add_space(22.0);
                ui.add(egui::Label::new(tk::hint(answer)).wrap());
            });
        }
    });
}
