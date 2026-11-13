use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, CornerRadius, Layout, Margin, Rect, RichText, Sense, Stroke};
use egui_phosphor::regular::{
    APP_WINDOW, ARROWS_CLOCKWISE, ARROW_LEFT, FILM_STRIP, FOLDER_OPEN, GAME_CONTROLLER, GAUGE, GEAR, KEYBOARD, PLAY,
    PLUS, SCISSORS, SPEAKER_HIGH, SQUARES_FOUR, TRASH, X,
};
use wasapi::Direction;

use crate::audio::list_devices;
use crate::capture::{RunState, Shared};
use crate::config::{AppConfig, CaptureTargetType, DesktopAudioMode, ExtraHotkey, GameProfile};
use crate::devices::{enumerate_monitors, enumerate_windows, MonitorInfo, WindowInfo};
use crate::hotkey::HotkeyRegistry;
use crate::library::{self, ClipEntry, Library};
use crate::tiksu::{self as tk, Kind, Size, Tone};
use crate::tray::{TrayAction, TrayManager};
use crate::update::{self, UpdateState, Updater};
use crate::win;

/// Unique process names of the open windows, i.e. apps whose audio or window can be recorded.
fn audio_apps(windows: &[WindowInfo]) -> Vec<String> {
    let mut apps: Vec<String> = windows.iter().map(|w| w.process_name.clone()).collect();
    apps.sort_by_key(|a| a.to_lowercase());
    apps.dedup();
    apps
}

/// Quality presets: (name, description, FPS, Mbps).
const QUALITY_PRESETS: [(&str, &str, u32, u32); 4] = [
    ("Kevyt", "Pienet tiedostot, kevyt kuorma.", 30, 8),
    ("Tasapaino", "Suositus useimpiin peleihin.", 60, 20),
    ("Terävä", "Nopea liike pysyy tarkkana.", 60, 40),
    ("Kilpapeli", "120 FPS nopeille näytöille.", 120, 60),
];

const CLIP_LENGTHS: [(u64, &str); 5] = [(30, "30 s"), (60, "1 min"), (120, "2 min"), (180, "3 min"), (300, "5 min")];
const EXTRA_LENGTHS: [(u64, &str); 5] = [(15, "15 s"), (30, "30 s"), (60, "1 min"), (120, "2 min"), (300, "5 min")];
const HOTKEYS: [&str; 12] = [
    "F7", "F8", "F9", "F10", "F11", "F12", "Alt+F8", "Alt+F9", "Alt+F10", "Ctrl+F9", "Ctrl+F10", "Ctrl+Shift+F10",
];
const MAX_EXTRA_HOTKEYS: usize = 3;

/// Megabytes per minute of video at `mbps`.
fn mb_per_minute(mbps: u32) -> u32 {
    mbps * 60 / 8
}

/// RAM the in-memory buffer needs: the longest clip plus the segment being recorded.
fn buffer_ram_mb(cfg: &AppConfig) -> u64 {
    cfg.bitrate as u64 / 8 * (cfg.max_clip_secs() + cfg.segment_duration_secs) / 1_000_000
}

fn fmt_time(secs: f64) -> String {
    let secs = secs.max(0.0);
    format!("{}:{:04.1}", (secs / 60.0) as u64, secs % 60.0)
}

fn fmt_size(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} Gt", b as f64 / (1u64 << 30) as f64),
        b => format!("{:.1} Mt", b as f64 / (1u64 << 20) as f64),
    }
}

fn fmt_secs(secs: u64) -> String {
    match secs {
        s if s >= 60 && s % 60 == 0 => format!("{} min", s / 60),
        s => format!("{} s", s),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Overview,
    Clips,
    Source,
    Audio,
    Quality,
    Hotkey,
    Profiles,
    App,
}

impl Page {
    const ALL: [Page; 8] =
        [Page::Overview, Page::Clips, Page::Source, Page::Audio, Page::Quality, Page::Hotkey, Page::Profiles, Page::App];

    fn label(self) -> &'static str {
        match self {
            Page::Overview => "Yleiskatsaus",
            Page::Clips => "Leikkeet",
            Page::Source => "Kuvauslähde",
            Page::Audio => "Äänet",
            Page::Quality => "Laatu",
            Page::Hotkey => "Pikanäppäimet",
            Page::Profiles => "Peliprofiilit",
            Page::App => "Sovellus",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Page::Overview => SQUARES_FOUR,
            Page::Clips => FILM_STRIP,
            Page::Source => APP_WINDOW,
            Page::Audio => SPEAKER_HIGH,
            Page::Quality => GAUGE,
            Page::Hotkey => KEYBOARD,
            Page::Profiles => GAME_CONTROLLER,
            Page::App => GEAR,
        }
    }
}

struct Toast {
    text: String,
    tone: Tone,
    shown_at: f64,
}

/// What fixes a problem listed on the overview.
enum Fix {
    Open(Page),
    RetryFfmpeg,
    /// Nothing to press: it fixes itself (e.g. a download in progress).
    Wait,
    RestartForUpdate,
}

enum ClipAction {
    Play(PathBuf),
    Edit(PathBuf),
    Folder(PathBuf),
    AskDelete(PathBuf),
    Delete(PathBuf),
}

/// Trim editor state; times in seconds.
struct Editor {
    path: PathBuf,
    start: f64,
    /// Set once the clip's duration is known.
    end: Option<f64>,
    /// Frame shown in the big preview.
    preview: f64,
}

pub struct SillyBorgApp {
    /// Settings being edited; `saved` is what the buffer runs with. A difference shows the save bar.
    config: AppConfig,
    saved: AppConfig,
    shared: Arc<Shared>,
    updater: Arc<Updater>,
    last_update_state: UpdateState,
    page: Page,
    gpu_name: String,
    monitors: Vec<MonitorInfo>,
    windows: Vec<WindowInfo>,
    microphones: Vec<String>,
    output_devices: Vec<String>,
    audio_apps: Vec<String>,
    tray: Option<TrayManager>,
    tray_rx: Receiver<TrayAction>,
    show_requested: Arc<AtomicBool>,
    hotkeys: Option<HotkeyRegistry>,
    hotkey_errors: Vec<String>,
    /// Hotkey button that is waiting for a key press, and why the last press was refused.
    capturing_hotkey: Option<egui::Id>,
    capture_error: Option<String>,
    autostart_error: Option<String>,
    toast: Option<Toast>,
    library: Library,
    editor: Option<Editor>,
    confirm_delete: Option<PathBuf>,
    quitting: bool,
    hidden: bool,
    told_about_tray: bool,
    /// Frames left to keep the window hidden when started with `--tray` (eframe shows it after the first paint).
    hide_frames: u8,
    last_state: RunState,
    /// Show a toast when the buffer (re)start in progress finishes.
    announce_start: bool,
}

impl SillyBorgApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        config: AppConfig,
        shared: Arc<Shared>,
        updater: Arc<Updater>,
        instance: win::SingleInstance,
        start_hidden: bool,
        just_updated: bool,
    ) -> Self {
        let ctx = cc.egui_ctx.clone();
        shared.set_repaint_context(ctx.clone());
        updater.set_repaint_context(ctx.clone());

        let (tray_tx, tray_rx) = channel();
        let tray = {
            let (shared, ctx) = (shared.clone(), ctx.clone());
            TrayManager::new(&config.hotkey, move |action| {
                if action == TrayAction::SaveClip {
                    shared.save_clip(0); // works while the window is hidden
                } else {
                    let _ = tray_tx.send(action);
                    ctx.request_repaint();
                }
            })
        };
        if let Err(e) = &tray {
            println!("Ilmaisinalueen kuvaketta ei voitu luoda: {}", e);
        }

        let show_requested = Arc::new(AtomicBool::new(false));
        {
            let (flag, ctx) = (show_requested.clone(), ctx.clone());
            instance.on_show_request(move || {
                flag.store(true, Ordering::SeqCst);
                ctx.request_repaint();
            });
        }

        let mut hotkeys = HotkeyRegistry::new(shared.trigger_sender());
        let hotkey_errors = match &mut hotkeys {
            Ok(h) => h.set(&config.all_hotkeys()),
            Err(e) => vec![format!("Pikanäppäimiä ei voitu alustaa: {}", e)],
        };
        for e in &hotkey_errors {
            println!("Pikanäppäin: {}", e);
        }

        let windows = enumerate_windows();
        Self {
            gpu_name: win::gpu_name(),
            monitors: enumerate_monitors(),
            audio_apps: audio_apps(&windows),
            windows,
            microphones: list_devices(Direction::Capture),
            output_devices: list_devices(Direction::Render),
            tray: tray.ok(),
            tray_rx,
            show_requested,
            hotkeys: hotkeys.ok(),
            hotkey_errors,
            capturing_hotkey: None,
            capture_error: None,
            autostart_error: None,
            toast: just_updated.then(|| Toast {
                text: format!("Tiksu Clipper päivitetty versioon {}", update::CURRENT),
                tone: Tone::Success,
                shown_at: 0.0,
            }),
            library: Library::new(ctx),
            last_update_state: UpdateState::Idle,
            updater,
            editor: None,
            confirm_delete: None,
            quitting: false,
            hidden: start_hidden,
            told_about_tray: start_hidden,
            hide_frames: if start_hidden { 3 } else { 0 },
            last_state: RunState::Starting,
            announce_start: false,
            page: Page::Overview,
            saved: config.clone(),
            config,
            shared,
        }
    }

    fn notify(&mut self, ctx: &egui::Context, tone: Tone, text: impl Into<String>) {
        self.toast = Some(Toast { text: text.into(), tone, shown_at: ctx.input(|i| i.time) });
    }

    fn refresh_devices(&mut self, ctx: &egui::Context) {
        self.monitors = enumerate_monitors();
        self.windows = enumerate_windows();
        self.audio_apps = audio_apps(&self.windows);
        self.microphones = list_devices(Direction::Capture);
        self.output_devices = list_devices(Direction::Render);
        self.notify(ctx, Tone::Success, "Laitteet ja ikkunat päivitetty");
    }

    /// Saves the edited settings; the supervisor applies them in the background.
    fn save_and_apply(&mut self, ctx: &egui::Context) {
        if let Err(e) = self.config.save() {
            self.notify(ctx, Tone::Danger, format!("Asetusten tallennus epäonnistui: {}", e));
            return;
        }
        let prev = std::mem::replace(&mut self.saved, self.config.clone());
        if let Some(hotkeys) = &mut self.hotkeys {
            self.hotkey_errors = hotkeys.set(&self.saved.all_hotkeys());
        }
        if let Some(tray) = &self.tray {
            tray.set_hotkey(&self.saved.hotkey);
        }
        if prev.autostart != self.saved.autostart {
            self.autostart_error = win::set_autostart(self.saved.autostart)
                .err()
                .map(|e| format!("Windowsin käynnistykseen lisääminen epäonnistui: {}", e));
        }
        self.updater.set_auto(self.saved.auto_update);
        let restarts = prev.video_differs(&self.saved);
        self.announce_start = restarts;
        self.shared.set_config(self.saved.clone());

        if !self.hotkey_errors.is_empty() || self.autostart_error.is_some() {
            self.notify(ctx, Tone::Warning, "Tallennettu, mutta osa vaatii huomiota. Katso Yleiskatsaus.");
        } else if restarts {
            self.notify(ctx, Tone::Success, "Tallennettu. Puskuri käynnistyy uusilla asetuksilla.");
        } else {
            self.notify(ctx, Tone::Success, "Asetukset tallennettu");
        }
    }

    /// Starts the installed new version and closes this one (the buffer restarts with it).
    fn restart_for_update(&mut self, ctx: &egui::Context) {
        match update::restart(self.hidden) {
            Ok(()) => {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => self.notify(ctx, Tone::Danger, format!("Uudelleenkäynnistys epäonnistui: {}", e)),
        }
    }

    fn open_output_dir(&self) {
        let _ = std::fs::create_dir_all(&self.saved.output_dir);
        let _ = std::process::Command::new("explorer").arg(&self.saved.output_dir).spawn();
    }

    fn save_clip(&mut self, ctx: &egui::Context) {
        self.shared.save_clip(0);
        self.notify(ctx, Tone::Info, format!("Tallennetaan viimeiset {}", fmt_secs(self.shared.live_config().clip_duration_secs)));
    }

    fn open_editor(&mut self, path: PathBuf) {
        self.editor = Some(Editor { path, start: 0.0, end: None, preview: 0.0 });
        self.page = Page::Clips;
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        self.hide_frames = 0;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        self.hidden = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        if !self.told_about_tray {
            self.told_about_tray = true;
            crate::overlay::show("Tiksu Clipper jatkaa taustalla", "Avaa ilmaisinalueen kuvakkeesta", true);
        }
    }

    // ------------------------------------------------------------------
    // Frame: sidebar, topbar
    // ------------------------------------------------------------------

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(24.0);
            ui.label(tk::display("Tiksu Clipper", 19.0).color(Color32::WHITE));
        });
        ui.add_space(28.0);

        for page in Page::ALL {
            let active = self.page == page;
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let width = ui.available_width() - 12.0;
                let (rect, resp) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, page.label()));
                let p = ui.painter();
                if active {
                    p.rect_filled(rect, CornerRadius::same(tk::R_SM), Color32::from_white_alpha(20));
                } else if resp.hovered() {
                    p.rect_filled(rect, CornerRadius::same(tk::R_SM), Color32::from_white_alpha(10));
                }
                let font = if active { tk::semibold_font(14.0) } else { egui::FontId::proportional(14.0) };
                p.text(rect.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, page.icon(), egui::FontId::proportional(18.0), Color32::WHITE);
                p.text(rect.left_center() + vec2(40.0, 0.0), Align2::LEFT_CENTER, page.label(), font, Color32::WHITE);
                if resp.has_focus() {
                    p.rect_stroke(rect.expand(2.0), CornerRadius::same(10), Stroke::new(2.0, tk::MINT), egui::StrokeKind::Outside);
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.page = page;
                }
            });
            ui.add_space(2.0);
        }
    }

    fn topbar(&mut self, ui: &mut egui::Ui) {
        let state = self.shared.status.lock().state.clone();
        let enabled = self.shared.is_enabled();
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new("Tiksu Clipper").size(13.0).color(tk::MUTED));
            ui.label(RichText::new("/").size(13.0).color(tk::MUTED));
            ui.label(RichText::new(self.page.label()).size(13.0).color(tk::TEXT_2));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = if enabled { "Pysäytä puskuri" } else { "Käynnistä puskuri" };
                if tk::button(ui, Kind::Secondary, Size::Sm, label, true).clicked() {
                    self.shared.set_enabled(!enabled);
                    self.announce_start = !enabled;
                }
                match &state {
                    RunState::Running => tk::badge(ui, Tone::Success, "Puskuri käynnissä"),
                    RunState::Starting => tk::badge(ui, Tone::Info, "Käynnistetään…"),
                    RunState::Stopped => tk::badge(ui, Tone::Neutral, "Puskuri pysäytetty"),
                    RunState::Failed(_) => tk::badge(ui, Tone::Danger, "Puskuri ei käynnistynyt"),
                };
            });
        });
    }

    fn page_header(&mut self, ui: &mut egui::Ui, desc: &str, actions: impl FnOnce(&mut Self, &mut egui::Ui)) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(tk::display(self.page.label(), 28.0));
                ui.label(RichText::new(desc).size(14.0).color(tk::TEXT_2));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| actions(self, ui));
        });
        ui.add_space(24.0);
    }

    // ------------------------------------------------------------------
    // Overview
    // ------------------------------------------------------------------

    fn overview(&mut self, ui: &mut egui::Ui) {
        let (segments, buffered, bytes) = self.shared.ring.stats();
        let status = self.shared.status.lock().clone();
        let cfg = self.shared.live_config();

        let desc = format!(
            "Puskuri pitää muistissa viimeiset {}. Paina {} tallentaaksesi ne leikkeeksi.",
            fmt_secs(cfg.clip_duration_secs),
            cfg.hotkey
        );
        self.page_header(ui, &desc, |app, ui| {
            if tk::button(ui, Kind::Primary, Size::Md, "Tallenna leike", true).clicked() {
                let ctx = ui.ctx().clone();
                app.save_clip(&ctx);
            }
            if tk::button(ui, Kind::Secondary, Size::Md, &format!("{}  Avaa kansio", FOLDER_OPEN), true).clicked() {
                app.open_output_dir();
            }
        });

        let memory = if cfg.buffer_in_memory {
            format!("{} segmenttiä · {} RAM", segments, fmt_size(bytes as u64))
        } else {
            format!("{} segmenttiä levyllä", segments)
        };
        let (source, source_sub, source_color) = match (&status.active_profile, status.fallback) {
            (_, true) => (status.target.clone(), "Ikkunaa ei löydy, kuvataan näyttöä".to_string(), tk::WARNING),
            (Some(exe), _) => (status.target.clone(), format!("Peliprofiili: {}", exe), tk::MINT),
            (None, _) => (
                status.target.clone(),
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
            (DesktopAudioMode::Device, mic) => {
                ("Laite".to_string(), format!("{}{}", cfg.output_device, if mic { " + mikki" } else { "" }))
            }
            (DesktopAudioMode::Application, mic) => {
                ("Sovellus".to_string(), format!("{}{}", cfg.app_audio_process, if mic { " + mikki" } else { "" }))
            }
        };
        let (hk_sub, hk_color) = if !self.hotkey_errors.is_empty() {
            ("Varattu toiselle ohjelmalle".to_string(), tk::WARNING)
        } else if !cfg.extra_hotkeys.is_empty() {
            (format!("+ {} lisänäppäintä", cfg.extra_hotkeys.len()), tk::MUTED)
        } else {
            ("Toimii myös peleissä".to_string(), tk::MUTED)
        };
        let stats = [
            ("Puskurissa", format!("{:.0} / {} s", buffered, cfg.max_clip_secs()), memory, tk::MUTED),
            ("Kuvauslähde", source, source_sub, source_color),
            ("Ääni", audio.0, audio.1, tk::MUTED),
            ("Pikanäppäin", cfg.hotkey.clone(), hk_sub, hk_color),
        ];
        let width = ui.available_width();
        let cols = ((width + 12.0) / (180.0 + 12.0)).floor().clamp(1.0, 4.0) as usize;
        let col_w = (width - 12.0 * (cols - 1) as f32) / cols as f32;
        for row in stats.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for (label, value, sub, color) in row {
                    tk::stat(ui, col_w, label, value, sub, *color);
                }
            });
            ui.add_space(4.0);
        }
        ui.add_space(20.0);

        // Problems first: they explain why something doesn't work.
        let sections: &[fn(&mut Self, &mut egui::Ui)] = if self.problems().is_empty() {
            &[Self::last_clip_card]
        } else {
            &[Self::problems_card, Self::last_clip_card]
        };
        tk::grid(ui, self, 380.0, sections);
    }

    fn problems(&self) -> Vec<(String, Fix)> {
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

    fn problems_card(&mut self, ui: &mut egui::Ui) {
        let problems = self.problems();
        tk::card(ui, |ui| {
            let count = format!("{} kpl", problems.len());
            tk::card_header_badge(ui, "Vaatii huomiota", "", Some((Tone::Warning, &count)));
            for (i, (text, fix)) in problems.iter().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                ui.add(egui::Label::new(RichText::new(text).size(13.0).color(tk::TEXT_2)).wrap());
                match fix {
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
                        ui.add(egui::Spinner::new().color(tk::MINT));
                    }
                }
            }
        });
    }

    fn last_clip_card(&mut self, ui: &mut egui::Ui) {
        let saving = self.shared.saving.load(Ordering::SeqCst);
        let last = self.shared.last_clip.lock().clone();
        let cfg = self.saved.clone();
        let mut action = None;
        tk::card(ui, |ui| {
            tk::card_header(ui, "Viimeisin leike", "Leikkeet tallentuvat MP4-tiedostoina tallennuskansioon.");
            if saving {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new().color(tk::MINT));
                    ui.label(RichText::new("Tallennetaan leikettä…").color(tk::TEXT_2));
                });
                return;
            }
            match last {
                Some(clip) => match clip.result {
                    Err(e) => {
                        tk::badge(ui, Tone::Danger, "Tallennus epäonnistui");
                        ui.add(egui::Label::new(RichText::new(e).size(13.0).color(tk::TEXT_2)).wrap());
                    }
                    Ok(path) => {
                        tk::badge(ui, Tone::Success, &format!("Tallennettu · {}", fmt_secs(clip.secs)));
                        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        ui.add(egui::Label::new(tk::mono(name, 13.0).color(tk::TEXT)).truncate());
                        ui.horizontal(|ui| {
                            if tk::button(ui, Kind::Secondary, Size::Sm, &format!("{} Toista", PLAY), true).clicked() {
                                action = Some(ClipAction::Play(path.clone()));
                            }
                            if tk::button(ui, Kind::Secondary, Size::Sm, &format!("{} Leikkaa", SCISSORS), true).clicked() {
                                action = Some(ClipAction::Edit(path.clone()));
                            }
                            if tk::button(ui, Kind::Ghost, Size::Sm, &format!("{} Kansio", FOLDER_OPEN), true).clicked() {
                                action = Some(ClipAction::Folder(path.clone()));
                            }
                        });
                    }
                },
                None => {
                    ui.label(RichText::new("Ei vielä leikkeitä tällä kertaa").font(tk::semibold_font(14.0)).color(tk::TEXT));
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!(
                                "Paina {} tai Tallenna leike, niin viimeiset {} tallentuvat. Aiemmat leikkeet löytyvät Leikkeet-sivulta.",
                                cfg.hotkey,
                                fmt_secs(cfg.clip_duration_secs)
                            ))
                            .size(13.0)
                            .color(tk::MUTED),
                        )
                        .wrap(),
                    );
                }
            }
        });
        if let Some(action) = action {
            self.clip_action(ui.ctx(), action);
        }
    }

    // ------------------------------------------------------------------
    // Clips: library and trim editor
    // ------------------------------------------------------------------

    fn clip_action(&mut self, ctx: &egui::Context, action: ClipAction) {
        match action {
            ClipAction::Play(path) => library::open(&path),
            ClipAction::Edit(path) => self.open_editor(path),
            ClipAction::Folder(path) => library::show_in_folder(&path),
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

    fn clips(&mut self, ui: &mut egui::Ui) {
        if self.editor.is_some() {
            return self.trim_editor(ui);
        }
        let saves = self.shared.clips_saved.load(Ordering::SeqCst);
        self.library.refresh(&self.saved.output_dir, saves, false);

        let mut rescan = false;
        self.page_header(ui, "Tallennetut leikkeet uusimmasta alkaen. Leikkaa, toista tai poista.", |app, ui| {
            if tk::button(ui, Kind::Secondary, Size::Md, &format!("{}  Avaa kansio", FOLDER_OPEN), true).clicked() {
                app.open_output_dir();
            }
            rescan = tk::button(ui, Kind::Ghost, Size::Md, &format!("{} Päivitä", ARROWS_CLOCKWISE), true).clicked();
        });
        if rescan {
            self.library.refresh(&self.saved.output_dir, saves, true);
        }

        let clips = self.library.clips.clone();
        if clips.is_empty() {
            tk::card(ui, |ui| {
                tk::card_header(ui, "Ei vielä leikkeitä", "");
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "Paina {} pelin aikana, niin leike ilmestyy tänne. Kansio: {}",
                            self.saved.hotkey,
                            self.saved.output_dir.display()
                        ))
                        .size(13.0)
                        .color(tk::MUTED),
                    )
                    .wrap(),
                );
            });
            return;
        }

        let n = (((ui.available_width() + 16.0) / (260.0 + 16.0)).floor() as usize).clamp(1, 4);
        let mut action = None;
        for row in clips.chunks(n) {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                ui.columns(n, |cols| {
                    for (col, clip) in cols.iter_mut().zip(row) {
                        col.with_layout(Layout::top_down(Align::Min), |ui| {
                            if let Some(a) = self.clip_card(ui, clip) {
                                action = Some(a);
                            }
                        });
                    }
                });
            });
            ui.add_space(16.0);
        }
        if let Some(action) = action {
            self.clip_action(ui.ctx(), action);
        }
    }

    fn clip_card(&mut self, ui: &mut egui::Ui, clip: &ClipEntry) -> Option<ClipAction> {
        let mut action = None;
        let duration = self.library.duration(&clip.path);
        let thumb = self.library.thumb(&clip.path, duration.map_or(1.0, |d| (d / 3.0).min(5.0)), 320, 180);
        let confirming = self.confirm_delete.as_deref() == Some(clip.path.as_path());
        tk::card(ui, |ui| {
            let w = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(vec2(w, w * 9.0 / 16.0), Sense::click());
            paint_frame(ui, rect, thumb.as_ref());
            if resp.hovered() {
                ui.painter().circle_filled(rect.center(), 24.0, Color32::from_black_alpha(160));
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, PLAY, egui::FontId::proportional(22.0), Color32::WHITE);
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Toista").clicked() {
                action = Some(ClipAction::Play(clip.path.clone()));
            }
            ui.add_space(4.0);
            ui.add(egui::Label::new(RichText::new(&clip.name).font(tk::semibold_font(14.0)).color(tk::TEXT)).truncate());
            let date = chrono::DateTime::<chrono::Local>::from(clip.modified).format("%-d.%-m. %H:%M");
            let length = duration.map_or("–".to_string(), fmt_time);
            ui.label(tk::mono(format!("{} · {} · {}", length, fmt_size(clip.size), date), 12.0).color(tk::MUTED));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if tk::button(ui, Kind::Secondary, Size::Sm, &format!("{} Leikkaa", SCISSORS), true).clicked() {
                    action = Some(ClipAction::Edit(clip.path.clone()));
                }
                if tk::button(ui, Kind::Ghost, Size::Sm, FOLDER_OPEN, true).on_hover_text("Näytä kansiossa").clicked() {
                    action = Some(ClipAction::Folder(clip.path.clone()));
                }
                if confirming {
                    if tk::button(ui, Kind::Secondary, Size::Sm, "Siirrä roskakoriin", true).clicked() {
                        action = Some(ClipAction::Delete(clip.path.clone()));
                    }
                } else if tk::button(ui, Kind::Ghost, Size::Sm, TRASH, true).on_hover_text("Poista").clicked() {
                    action = Some(ClipAction::AskDelete(clip.path.clone()));
                }
            });
        });
        action
    }

    fn trim_editor(&mut self, ui: &mut egui::Ui) {
        let Some(mut ed) = self.editor.take() else { return };
        let mut close = false;
        let name = ed.path.file_name().unwrap_or_default().to_string_lossy().to_string();

        ui.horizontal(|ui| {
            close = tk::button(ui, Kind::Ghost, Size::Md, &format!("{} Leikkeet", ARROW_LEFT), true).clicked();
            ui.label(tk::display(format!("Leikkaa: {}", name), 22.0));
        });
        ui.add_space(16.0);

        let Some(duration) = self.library.duration(&ed.path) else {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().color(tk::MINT));
                ui.label(RichText::new("Luetaan leikettä…").color(tk::TEXT_2));
            });
            if !close {
                self.editor = Some(ed);
            }
            return;
        };
        let mut end = ed.end.unwrap_or(duration).min(duration);

        tk::card(ui, |ui| {
            // Big preview of the frame under the playhead.
            let w = ui.available_width().min(640.0);
            let preview = self
                .library
                .thumb(&ed.path, ed.preview, 640, 360)
                .or_else(|| self.library.nearest_thumb(&ed.path, ed.preview, 640, 360))
                .or_else(|| self.library.nearest_thumb(&ed.path, ed.preview, 160, 90));
            let (rect, _) = ui.allocate_exact_size(vec2(w, w * 9.0 / 16.0), Sense::hover());
            paint_frame(ui, rect, preview.as_ref());
            ui.painter().text(
                rect.left_bottom() + vec2(12.0, -12.0),
                Align2::LEFT_BOTTOM,
                fmt_time(ed.preview),
                tk::mono_font(13.0),
                Color32::WHITE,
            );
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
                Stroke::new(2.0, tk::MINT),
                egui::StrokeKind::Inside,
            );
            if strip_resp.clicked()
                && let Some(pos) = strip_resp.interact_pointer_pos() {
                    ed.preview = t_of(pos.x);
                }
            let min_len = 0.5f64.min(duration);
            for (which, t) in [(0, ed.start), (1, end)] {
                let handle = Rect::from_center_size(pos2(x_of(t), strip.center().y), vec2(10.0, strip.height() + 8.0));
                let resp = ui.interact(handle.expand2(vec2(6.0, 0.0)), ui.id().with(("trim_handle", which)), Sense::drag());
                let color = if resp.hovered() || resp.dragged() { Color32::WHITE } else { tk::MINT };
                ui.painter().rect_filled(handle, CornerRadius::same(3), color);
                if resp.dragged()
                    && let Some(pos) = resp.interact_pointer_pos() {
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
            ui.add_space(12.0);

            // Exact values.
            ui.horizontal(|ui| {
                ui.label(RichText::new("Alku").font(tk::semibold_font(13.0)).color(tk::TEXT_2));
                let max_start = (end - min_len).max(0.0);
                if ui
                    .add(egui::DragValue::new(&mut ed.start).speed(0.05).range(0.0..=max_start).suffix(" s").fixed_decimals(1))
                    .changed()
                {
                    ed.preview = ed.start;
                }
                ui.add_space(12.0);
                ui.label(RichText::new("Loppu").font(tk::semibold_font(13.0)).color(tk::TEXT_2));
                if ui
                    .add(egui::DragValue::new(&mut end).speed(0.05).range((ed.start + min_len)..=duration).suffix(" s").fixed_decimals(1))
                    .changed()
                {
                    ed.preview = end;
                }
                ui.add_space(12.0);
                ui.label(tk::mono(format!("Pituus {:.1} s / {:.1} s", end - ed.start, duration), 13.0).color(tk::TEXT));
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let busy = self.library.trimming;
                if tk::button(ui, Kind::Primary, Size::Md, &format!("{} Tallenna leikattu kopio", SCISSORS), !busy).clicked() {
                    self.library.trim(&ed.path, ed.start, end);
                }
                if tk::button(ui, Kind::Secondary, Size::Md, &format!("{} Toista alkuperäinen", PLAY), true).clicked() {
                    library::open(&ed.path);
                }
                if busy {
                    ui.add(egui::Spinner::new().color(tk::MINT));
                    ui.label(RichText::new("Leikataan…").color(tk::TEXT_2));
                }
            });
            ui.label(
                RichText::new("Alkuperäinen leike säilyy. Leikattu kopio tallentuu samaan kansioon nimellä …_trim.mp4.")
                    .size(12.0)
                    .color(tk::MUTED),
            );
        });

        ed.end = Some(end);
        if !close {
            self.editor = Some(ed);
        }
    }

    // ------------------------------------------------------------------
    // Source
    // ------------------------------------------------------------------

    fn source(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "Mitä puskuri kuvaa: koko näyttö tai yksi sovellusikkuna.", |_, _| {});
        tk::grid(ui, self, 340.0, &[Self::target_card, Self::target_detail_card]);
    }

    fn target_card(&mut self, ui: &mut egui::Ui) {
        tk::card(ui, |ui| {
            tk::card_header(ui, "Kuvattava kohde", "Valitse koko näyttö tai yksittäinen peli tai sovellus.");
            tk::segmented(
                ui,
                &mut self.config.capture_source_type,
                &[(CaptureTargetType::Monitor, "Koko näyttö"), (CaptureTargetType::Window, "Sovellusikkuna")],
            );
            ui.add_space(4.0);
            let hint = match self.config.capture_source_type {
                CaptureTargetType::Monitor => "Kaikki näytöllä näkyvä tallentuu, myös ilmoitukset ja muut ikkunat.",
                CaptureTargetType::Window => "Vain valittu ikkuna tallentuu, vaikka se jäisi muiden ikkunoiden taakse.",
            };
            ui.add(egui::Label::new(RichText::new(hint).size(12.0).color(tk::MUTED)).wrap());
        });
    }

    fn target_detail_card(&mut self, ui: &mut egui::Ui) {
        let mut refresh = false;
        tk::card(ui, |ui| match self.config.capture_source_type {
            CaptureTargetType::Monitor => {
                tk::card_header(ui, "Näyttö", "Usean näytön koneessa valitse, mitä niistä kuvataan.");
                let label = |m: &MonitorInfo| {
                    format!("Näyttö {}: {} ({}×{}){}", m.index, m.name, m.width, m.height, if m.is_primary { " · ensisijainen" } else { "" })
                };
                let selected = self
                    .monitors
                    .iter()
                    .find(|m| m.index == self.config.monitor_index)
                    .map(label)
                    .unwrap_or_else(|| "Valitse näyttö".to_string());
                egui::ComboBox::from_id_salt("monitor")
                    .selected_text(selected)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for m in &self.monitors {
                            ui.selectable_value(&mut self.config.monitor_index, m.index, label(m));
                        }
                    });
            }
            CaptureTargetType::Window => {
                tk::card_header(
                    ui,
                    "Ikkuna",
                    "Ikkuna löytyy sovelluksen nimellä, vaikka otsikko vaihtuisi. Jos sitä ei ole auki, kuvataan näyttöä kunnes se avautuu.",
                );
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
            }
        });
        if refresh {
            self.refresh_devices(ui.ctx());
        }
    }

    // ------------------------------------------------------------------
    // Audio
    // ------------------------------------------------------------------

    fn audio(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "Tietokoneen äänet ja mikrofoni tallentuvat leikkeeseen.", |_, _| {});
        let warnings = self.shared.status.lock().audio_warnings.clone();
        for w in &warnings {
            ui.horizontal(|ui| {
                tk::badge(ui, Tone::Warning, "Huomio");
                ui.add(egui::Label::new(RichText::new(w).size(13.0).color(tk::TEXT_2)).wrap());
            });
        }
        if !warnings.is_empty() {
            ui.add_space(12.0);
        }
        tk::grid(ui, self, 340.0, &[Self::desktop_audio_card, Self::mic_card, Self::tracks_card]);
    }

    fn device_picker(ui: &mut egui::Ui, id: &str, value: &mut String, options: &[String], placeholder: &str) -> bool {
        let mut refresh = false;
        ui.horizontal(|ui| {
            let text = if value.is_empty() { placeholder.to_string() } else { value.clone() };
            egui::ComboBox::from_id_salt(id)
                .selected_text(text)
                .width(ui.available_width() - 110.0)
                .show_ui(ui, |ui| {
                    for o in options {
                        ui.selectable_value(value, o.clone(), o);
                    }
                });
            refresh = tk::button(ui, Kind::Ghost, Size::Md, &format!("{} Päivitä", ARROWS_CLOCKWISE), true).clicked();
        });
        refresh
    }

    fn desktop_audio_card(&mut self, ui: &mut egui::Ui) {
        let mut refresh = false;
        tk::card(ui, |ui| {
            tk::card_header(ui, "Tietokoneen äänet", "Kaikki äänilaitteen äänet tai vain yhden sovelluksen äänet.");
            tk::segmented(
                ui,
                &mut self.config.desktop_audio,
                &[
                    (DesktopAudioMode::Off, "Ei ääniä"),
                    (DesktopAudioMode::Device, "Äänilähde"),
                    (DesktopAudioMode::Application, "Vain sovellus"),
                ],
            );
            ui.add_space(8.0);
            match self.config.desktop_audio {
                DesktopAudioMode::Off => {
                    ui.label(RichText::new("Pelin ja sovellusten äänet eivät tallennu.").size(12.0).color(tk::MUTED));
                }
                DesktopAudioMode::Device => {
                    tk::field(ui, "Äänilähde", "Tallentaa kaiken, mitä valitusta laitteesta kuuluu.", |ui| {
                        refresh = Self::device_picker(ui, "output", &mut self.config.output_device, &self.output_devices, "");
                    });
                }
                DesktopAudioMode::Application => {
                    tk::field(ui, "Sovellus", "Vain tämän sovelluksen äänet, esim. peli tai Discord.", |ui| {
                        refresh = Self::device_picker(ui, "app_audio", &mut self.config.app_audio_process, &self.audio_apps, "Valitse sovellus");
                    });
                }
            }
        });
        if refresh {
            self.refresh_devices(ui.ctx());
        }
    }

    fn mic_card(&mut self, ui: &mut egui::Ui) {
        let mut refresh = false;
        tk::card(ui, |ui| {
            tk::card_header(ui, "Mikrofoni", "Oma äänesi leikkeeseen pelin äänten kanssa.");
            tk::toggle_row(ui, "Tallenna mikrofoni", "Pois päältä leikkeessä kuuluvat vain tietokoneen äänet.", &mut self.config.record_microphone);
            if self.config.record_microphone {
                ui.add_space(8.0);
                tk::field(ui, "Mikrofonilaite", "", |ui| {
                    refresh = Self::device_picker(ui, "mic", &mut self.config.microphone_device, &self.microphones, "");
                });
            }
        });
        if refresh {
            self.refresh_devices(ui.ctx());
        }
    }

    fn tracks_card(&mut self, ui: &mut egui::Ui) {
        tk::card(ui, |ui| {
            tk::card_header(ui, "Ääniraidat", "Ensimmäisellä raidalla on aina kaikki äänet yhdessä.");
            tk::toggle_row(
                ui,
                "Erilliset raidat",
                "Lisäksi tietokone ja mikrofoni omille raidoilleen editointia varten.",
                &mut self.config.separate_audio_tracks,
            );
            if self.config.separate_audio_tracks && !(self.config.record_microphone && self.config.desktop_audio != DesktopAudioMode::Off) {
                ui.add(
                    egui::Label::new(
                        RichText::new("Erillisiä raitoja tulee vasta, kun sekä tietokoneen äänet että mikrofoni ovat päällä.")
                            .size(12.0)
                            .color(tk::WARNING),
                    )
                    .wrap(),
                );
            }
        });
    }

    // ------------------------------------------------------------------
    // Quality
    // ------------------------------------------------------------------

    fn quality(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "Valitse laatuprofiili tai säädä pituus, kuvanopeus ja bittinopeus itse.", |_, _| {});
        self.presets_card(ui);
        ui.add_space(16.0);
        tk::grid(ui, self, 380.0, &[Self::clip_length_card, Self::video_card, Self::buffer_card, Self::encoder_card]);
    }

    fn presets_card(&mut self, ui: &mut egui::Ui) {
        let current = QUALITY_PRESETS
            .iter()
            .position(|p| p.2 == self.config.frame_rate && p.3 * 1_000_000 == self.config.bitrate);
        tk::card(ui, |ui| {
            tk::card_header_badge(
                ui,
                "Laatuprofiili",
                "Profiili asettaa kuvanopeuden ja bittinopeuden. Koko on arvio minuutista videota.",
                current.is_none().then_some((Tone::Neutral, "Mukautettu")),
            );
            // 4, 2x2 or 1 per row: never an orphan tile.
            let n = match ui.available_width() {
                w if w >= 4.0 * 190.0 + 36.0 => 4,
                w if w >= 2.0 * 190.0 + 12.0 => 2,
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
                ui.add_space(4.0);
            }
        });
    }

    fn clip_length_card(&mut self, ui: &mut egui::Ui) {
        tk::card(ui, |ui| {
            tk::card_header(ui, "Leikkeen pituus", "Kuinka pitkä pätkä menneisyydestä tallentuu pääpikanäppäimellä.");
            // A slider value between presets simply leaves no chip selected.
            tk::segmented(ui, &mut self.config.clip_duration_secs, &CLIP_LENGTHS);
            ui.add_space(8.0);
            tk::field(ui, "Tarkka pituus", "", |ui| {
                ui.horizontal(|ui| {
                    let mut secs = self.config.clip_duration_secs as u32;
                    if tk::slider(ui, &mut secs, 15..=600, 5).changed() {
                        self.config.clip_duration_secs = secs as u64;
                    }
                    ui.label(tk::mono(format!("{} s", self.config.clip_duration_secs), 13.0).color(tk::TEXT));
                });
            });
        });
    }

    fn video_card(&mut self, ui: &mut egui::Ui) {
        tk::card(ui, |ui| {
            tk::card_header(ui, "Kuva", "Kuvanopeus ja bittinopeus käsin.");
            tk::field(ui, "Kuvanopeus", "", |ui| {
                tk::segmented(ui, &mut self.config.frame_rate, &[(30, "30 FPS"), (60, "60 FPS"), (120, "120 FPS")]);
            });
            ui.add_space(8.0);
            let mut mbps = self.config.bitrate / 1_000_000;
            let hint = format!("Noin {} Mt minuutissa videota.", mb_per_minute(mbps));
            tk::field(ui, "Bittinopeus", &hint, |ui| {
                ui.horizontal(|ui| {
                    if tk::slider(ui, &mut mbps, 5..=80, 1).changed() {
                        self.config.bitrate = mbps * 1_000_000;
                    }
                    ui.label(tk::mono(format!("{} Mbps", mbps), 13.0).color(tk::TEXT));
                });
            });
        });
    }

    fn buffer_card(&mut self, ui: &mut egui::Ui) {
        tk::card(ui, |ui| {
            tk::card_header(ui, "Puskurin sijainti", "Missä viimeiset minuutit odottavat tallennusta.");
            tk::segmented(ui, &mut self.config.buffer_in_memory, &[(true, "Muistissa"), (false, "Levyllä")]);
            ui.add_space(4.0);
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
            ui.add(egui::Label::new(RichText::new(hint).size(12.0).color(tk::MUTED)).wrap());
            // More than half of the free RAM: the buffer would starve the game (or Windows).
            let free_mb = win::available_ram_bytes().map(|b| b / 1_000_000);
            if let Some(free_mb) = free_mb.filter(|free| self.config.buffer_in_memory && buffer_ram_mb(&self.config) > free / 2) {
                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("Muistia on vapaana vain {} Mt. Valitse lyhyempi leike, pienempi bittinopeus tai puskuri levylle.", free_mb))
                            .size(12.0)
                            .color(tk::DANGER),
                    )
                    .wrap(),
                );
            }
        });
    }

    fn encoder_card(&mut self, ui: &mut egui::Ui) {
        tk::card(ui, |ui| {
            tk::card_header(ui, "Enkooderi", "Näytönohjain koodaa videon, jolloin peli pysyy sulavana.");
            tk::badge(ui, Tone::Info, &format!("Näytönohjain: {}", self.gpu_name));
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "H.264 Windowsin Media Foundationin kautta. Windows valitsee laitteistoenkooderin (NVENC, AMF tai Quick Sync) automaattisesti.",
                    )
                    .size(12.0)
                    .color(tk::MUTED),
                )
                .wrap(),
            );
        });
    }

    // ------------------------------------------------------------------
    // Hotkeys and folder
    // ------------------------------------------------------------------

    fn hotkey(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "Millä näppäimillä leike tallennetaan ja minne.", |_, _| {});
        tk::grid(ui, self, 340.0, &[Self::hotkey_card, Self::extra_hotkeys_card, Self::folder_card]);
    }

    /// A button showing the key; clicking it listens for the next key press (Esc cancels).
    fn hotkey_picker(ui: &mut egui::Ui, id: egui::Id, value: &mut String, capturing: &mut Option<egui::Id>, error: &mut Option<String>) {
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
        if let Some(e) = error.as_ref().filter(|_| capturing.is_some()) {
            ui.add(egui::Label::new(RichText::new(e).size(12.0).color(tk::DANGER)).wrap());
        } else {
            ui.label(RichText::new("Esc peruu.").size(12.0).color(tk::MUTED));
        }
    }

    fn hotkey_card(&mut self, ui: &mut egui::Ui) {
        tk::card(ui, |ui| {
            tk::card_header(ui, "Pääpikanäppäin", "Toimii myös koko näytön peleissä.");
            Self::hotkey_picker(ui, egui::Id::new("hotkey"), &mut self.config.hotkey, &mut self.capturing_hotkey, &mut self.capture_error);
            ui.label(
                RichText::new(format!("Tallentaa {} (Laatu-sivun leikkeen pituus).", fmt_secs(self.config.clip_duration_secs)))
                    .size(12.0)
                    .color(tk::MUTED),
            );
            for err in &self.hotkey_errors {
                ui.add(egui::Label::new(RichText::new(err).size(12.0).color(tk::DANGER)).wrap());
            }
        });
    }

    fn extra_hotkeys_card(&mut self, ui: &mut egui::Ui) {
        tk::card(ui, |ui| {
            tk::card_header(ui, "Lisäpikanäppäimet", "Eri näppäin eri pituiselle leikkeelle, esim. nopea 15 s.");
            let mut remove = None;
            for (i, extra) in self.config.extra_hotkeys.iter_mut().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    Self::hotkey_picker(ui, egui::Id::new(("extra_hotkey", i)), &mut extra.hotkey, &mut self.capturing_hotkey, &mut self.capture_error);
                    if tk::button(ui, Kind::Ghost, Size::Sm, TRASH, true).on_hover_text("Poista").clicked() {
                        remove = Some(i);
                    }
                });
                tk::segmented(ui, &mut extra.secs, &EXTRA_LENGTHS);
            }
            if let Some(i) = remove {
                self.config.extra_hotkeys.remove(i);
            }
            if self.config.extra_hotkeys.len() < MAX_EXTRA_HOTKEYS {
                ui.add_space(4.0);
                if tk::button(ui, Kind::Secondary, Size::Sm, &format!("{} Lisää pikanäppäin", PLUS), true).clicked() {
                    let used: Vec<String> = self.config.all_hotkeys().into_iter().map(|(k, _)| k).collect();
                    let key = HOTKEYS.iter().find(|k| !used.iter().any(|u| u == *k)).unwrap_or(&"F9");
                    self.config.extra_hotkeys.push(ExtraHotkey { hotkey: key.to_string(), secs: 30 });
                }
            }
        });
    }

    fn folder_card(&mut self, ui: &mut egui::Ui) {
        tk::card(ui, |ui| {
            tk::card_header(ui, "Tallennuskansio", "Valmiit leikkeet tallentuvat tänne.");
            egui::Frame::new()
                .fill(tk::SURFACE_SUNK)
                .stroke(Stroke::new(1.0, tk::LINE))
                .corner_radius(CornerRadius::same(tk::R_MD))
                .inner_margin(Margin::symmetric(12, 11))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(tk::mono(self.config.output_dir.to_string_lossy(), 13.0).color(tk::TEXT)).truncate());
                });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if tk::button(ui, Kind::Secondary, Size::Md, "Selaa…", true).clicked()
                    && let Some(folder) = rfd::FileDialog::new().set_directory(&self.config.output_dir).pick_folder() {
                        self.config.output_dir = folder;
                    }
                if tk::button(ui, Kind::Ghost, Size::Md, &format!("{} Avaa kansio", FOLDER_OPEN), true).clicked() {
                    self.open_output_dir();
                }
            });
        });
    }

    // ------------------------------------------------------------------
    // Game profiles
    // ------------------------------------------------------------------

    fn profiles(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "Kun peli on käynnissä, sen omat asetukset otetaan käyttöön automaattisesti.", |_, _| {});
        let active = self.shared.status.lock().active_profile.clone();
        let mut refresh = false;

        tk::card(ui, |ui| {
            tk::card_header(ui, "Automaattinen vaihto", "Profiili vaihtuu, kun peli tulee etualalle, ja palautuu, kun peli suljetaan.");
            tk::toggle_row(ui, "Käytä peliprofiileja", "Pois päältä käytetään aina perusasetuksia.", &mut self.config.auto_profiles);
            ui.add_space(8.0);
            tk::field(ui, "Lisää peli", "Lista näyttää nyt auki olevat sovellukset. Käynnistä peli ja päivitä.", |ui| {
                ui.horizontal(|ui| {
                    let available: Vec<String> = self
                        .audio_apps
                        .iter()
                        .filter(|a| self.config.profile_for(a).is_none())
                        .cloned()
                        .collect();
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
        if refresh {
            self.refresh_devices(ui.ctx());
        }
        ui.add_space(16.0);

        let n = if ui.available_width() >= 2.0 * 380.0 + 16.0 { 2 } else { 1 };
        let mut remove = None;
        let count = self.config.profiles.len();
        for start in (0..count).step_by(n) {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                ui.columns(n, |cols| {
                    for (j, col) in cols.iter_mut().enumerate() {
                        let i = start + j;
                        if i >= count {
                            break;
                        }
                        col.with_layout(Layout::top_down(Align::Min), |ui| {
                            let p = &mut self.config.profiles[i];
                            let is_active = active.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(&p.exe));
                            if profile_card(ui, i, p, is_active) {
                                remove = Some(i);
                            }
                        });
                    }
                });
            });
            ui.add_space(16.0);
        }
        if let Some(i) = remove {
            self.config.profiles.remove(i);
        }
    }

    // ------------------------------------------------------------------
    // App
    // ------------------------------------------------------------------

    fn app(&mut self, ui: &mut egui::Ui) {
        self.page_header(ui, "Käynnistys, taustalla toiminta ja ilmoitukset.", |_, _| {});
        tk::grid(ui, self, 360.0, &[Self::update_card, Self::startup_card, Self::notify_card, Self::info_card]);
    }

    fn update_card(&mut self, ui: &mut egui::Ui) {
        let state = self.updater.state();
        let busy = matches!(state, UpdateState::Checking | UpdateState::Downloading(_));
        let mut restart = false;
        tk::card(ui, |ui| {
            tk::card_header(ui, "Päivitykset", "Uudet versiot GitHubista ja tiksu.fi:stä. Vain Tiksun allekirjoittamat versiot asennetaan.");
            tk::toggle_row(
                ui,
                "Päivitä automaattisesti",
                "Lataa ja asentaa taustalla. Uusi versio käynnistyy seuraavalla kerralla.",
                &mut self.config.auto_update,
            );
            ui.separator();
            ui.label(tk::mono(format!("Nykyinen versio {}", update::CURRENT), 13.0).color(tk::TEXT));
            let notes = |ui: &mut egui::Ui, notes: &str| {
                if !notes.is_empty() {
                    ui.add(egui::Label::new(RichText::new(notes).size(12.0).color(tk::TEXT_2)).wrap());
                }
            };
            match &state {
                UpdateState::Idle => {
                    ui.label(RichText::new("Ei vielä tarkistettu tällä käynnistyksellä.").size(12.0).color(tk::MUTED));
                }
                UpdateState::Checking | UpdateState::Downloading(_) => {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().color(tk::MINT));
                        let text = match &state {
                            UpdateState::Downloading(m) => format!("Ladataan versiota {}…", m.version),
                            _ => "Tarkistetaan…".to_string(),
                        };
                        ui.label(RichText::new(text).color(tk::TEXT_2));
                    });
                }
                UpdateState::UpToDate => {
                    tk::badge(ui, Tone::Success, "Ajan tasalla");
                }
                UpdateState::Available(m) => {
                    tk::badge(ui, Tone::Info, &format!("Uusi versio {}", m.version));
                    notes(ui, &m.notes);
                    if tk::button(ui, Kind::Primary, Size::Sm, "Lataa ja asenna", true).clicked() {
                        self.updater.check(true);
                    }
                }
                UpdateState::Installed(m) => {
                    tk::badge(ui, Tone::Success, &format!("Versio {} asennettu", m.version));
                    notes(ui, &m.notes);
                    restart = tk::button(ui, Kind::Primary, Size::Sm, "Käynnistä uudelleen nyt", true).clicked();
                }
                UpdateState::Failed(e) => {
                    tk::badge(ui, Tone::Danger, "Päivitys epäonnistui");
                    ui.add(egui::Label::new(RichText::new(e).size(12.0).color(tk::TEXT_2)).wrap());
                }
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if tk::button(ui, Kind::Secondary, Size::Sm, "Tarkista nyt", !busy).clicked() {
                    self.updater.check(self.saved.auto_update);
                }
                if tk::button(ui, Kind::Ghost, Size::Sm, "Lataussivu", true).clicked() {
                    let _ = std::process::Command::new("explorer").arg(update::DOWNLOAD_PAGE).spawn();
                }
            });
        });
        if restart {
            self.restart_for_update(ui.ctx());
        }
    }

    fn startup_card(&mut self, ui: &mut egui::Ui) {
        tk::card(ui, |ui| {
            tk::card_header(ui, "Käynnistys", "");
            tk::toggle_row(ui, "Käynnistä Windowsin mukana", "Aukeaa suoraan ilmaisinalueelle, puskuri päällä.", &mut self.config.autostart);
            ui.separator();
            tk::toggle_row(ui, "Sulje ilmaisinalueelle", "Ikkunan sulkeminen jättää puskurin käyntiin taustalle.", &mut self.config.close_to_tray);
        });
    }

    fn notify_card(&mut self, ui: &mut egui::Ui) {
        tk::card(ui, |ui| {
            tk::card_header(ui, "Ilmoitukset", "Palaute tallennuksesta kesken pelin.");
            tk::toggle_row(ui, "Ilmoitus ruudun kulmassa", "Näkyy ikkunoitujen ja reunattomien pelien päällä.", &mut self.config.overlay);
            ui.separator();
            tk::toggle_row(ui, "Merkkiääni", "Eri ääni onnistumiselle ja virheelle.", &mut self.config.play_sound);
            ui.add_space(4.0);
            if tk::button(ui, Kind::Secondary, Size::Sm, "Kokeile ilmoitusta", true).clicked() {
                if self.config.overlay {
                    crate::overlay::show("Leike tallennettu", "30 s · esimerkki.mp4", true);
                }
                if self.config.play_sound {
                    crate::notification::play_chime(true);
                }
            }
        });
    }

    fn info_card(&mut self, ui: &mut egui::Ui) {
        tk::card(ui, |ui| {
            tk::card_header(ui, "Tietoja", "");
            ui.label(tk::mono(format!("Tiksu Clipper {}", env!("CARGO_PKG_VERSION")), 13.0).color(tk::TEXT));
            ui.label(RichText::new(format!("Näytönohjain: {}", self.gpu_name)).size(12.0).color(tk::MUTED));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if tk::button(ui, Kind::Secondary, Size::Sm, "Avaa loki", true).clicked() {
                    let _ = std::process::Command::new("notepad").arg(crate::applog::path()).spawn();
                }
                if tk::button(ui, Kind::Ghost, Size::Sm, "Asetuskansio", true).clicked()
                    && let Some(dir) = AppConfig::config_path().parent() {
                        let _ = std::process::Command::new("explorer").arg(dir).spawn();
                    }
            });
        });
    }

    // ------------------------------------------------------------------
    // Floating: save bar, toast
    // ------------------------------------------------------------------

    fn save_bar(&mut self, ctx: &egui::Context) {
        if self.config == self.saved {
            return;
        }
        egui::Area::new(egui::Id::new("save_bar"))
            .anchor(Align2::CENTER_BOTTOM, vec2(tk::SIDEBAR_W / 2.0, -24.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(tk::OVERLAY)
                    .stroke(Stroke::new(1.0, tk::LINE_BRAND))
                    .corner_radius(CornerRadius::same(255))
                    .inner_margin(Margin { left: 20, right: 8, top: 8, bottom: 8 })
                    .shadow(ui.visuals().popup_shadow)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (dot, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                            ui.painter().circle_filled(dot.center(), 4.0, tk::MINT);
                            ui.label(RichText::new("Tallentamattomia muutoksia").color(tk::TEXT));
                            ui.add_space(16.0);
                            if tk::button(ui, Kind::Ghost, Size::Sm, "Peru", true).clicked() {
                                self.config = self.saved.clone();
                            }
                            if tk::button(ui, Kind::Primary, Size::Sm, "Tallenna muutokset", true).clicked() {
                                self.save_and_apply(ctx);
                            }
                        });
                    });
            });
    }

    /// Top right under the topbar, 3px colored left edge. Disappears after 5 s, errors stay.
    fn toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = &self.toast else { return };
        let now = ctx.input(|i| i.time);
        let sticky = matches!(toast.tone, Tone::Danger | Tone::Warning);
        if !sticky && now - toast.shown_at > 5.0 {
            self.toast = None;
            return;
        }
        if !sticky {
            ctx.request_repaint_after(Duration::from_secs_f64((5.0 - (now - toast.shown_at)).max(0.05)));
        }
        let mut close = false;
        egui::Area::new(egui::Id::new("toast"))
            .anchor(Align2::RIGHT_TOP, vec2(-16.0, tk::TOPBAR_H + 16.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let resp = egui::Frame::new()
                    .fill(tk::OVERLAY)
                    .stroke(Stroke::new(1.0, tk::LINE_STRONG))
                    .corner_radius(CornerRadius::same(tk::R_MD))
                    .inner_margin(Margin { left: 19, right: 8, top: 10, bottom: 10 })
                    .shadow(ui.visuals().popup_shadow)
                    .show(ui, |ui| {
                        ui.set_max_width(380.0);
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(RichText::new(&toast.text).size(14.0).color(tk::TEXT)).wrap());
                            if sticky {
                                close = tk::button(ui, Kind::Ghost, Size::Sm, X, true).on_hover_text("Sulje").clicked();
                            }
                        });
                    })
                    .response;
                let r = resp.rect;
                let edge = Rect::from_min_max(r.left_top(), pos2(r.left() + 3.0, r.bottom()));
                ui.painter().rect_filled(edge, CornerRadius { nw: tk::R_MD, sw: tk::R_MD, ne: 0, se: 0 }, toast.tone.color());
            });
        if close {
            self.toast = None;
        }
    }
}

/// A video frame (or a dark placeholder while it loads) filling `rect`.
fn paint_frame(ui: &egui::Ui, rect: Rect, tex: Option<&egui::TextureHandle>) {
    let p = ui.painter();
    match tex {
        Some(tex) => {
            p.image(tex.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            p.rect_filled(rect, CornerRadius::same(tk::R_SM), tk::SURFACE_SUNK);
            p.text(rect.center(), Align2::CENTER_CENTER, FILM_STRIP, egui::FontId::proportional(20.0), tk::LINE_STRONG);
        }
    }
}

/// One game profile's settings. Returns true when the user removes it.
fn profile_card(ui: &mut egui::Ui, index: usize, p: &mut GameProfile, active: bool) -> bool {
    let mut remove = false;
    tk::card(ui, |ui| {
        tk::card_header_badge(ui, &p.exe, "", active.then_some((Tone::Success, "Käytössä nyt")));
        tk::toggle_row(ui, "Kuvaa vain pelin ikkuna", "Pois päältä käytetään perusasetusten kuvauslähdettä.", &mut p.capture_window);
        ui.add_space(4.0);
        tk::field(ui, "Laatu", "", |ui| {
            let mut quality = (p.frame_rate, p.bitrate / 1_000_000);
            let options: Vec<((u32, u32), &str)> = QUALITY_PRESETS.iter().map(|(name, _, fps, mbps)| ((*fps, *mbps), *name)).collect();
            if tk::segmented(ui, &mut quality, &options) {
                p.frame_rate = quality.0;
                p.bitrate = quality.1 * 1_000_000;
            }
        });
        tk::field(ui, "Leikkeen pituus", "", |ui| {
            tk::segmented(ui, &mut p.clip_duration_secs, &CLIP_LENGTHS);
        });
        ui.add_space(4.0);
        ui.push_id(index, |ui| {
            remove = tk::button(ui, Kind::Ghost, Size::Sm, &format!("{} Poista profiili", TRASH), true).clicked();
        });
    });
    remove
}

fn handle_tray(app: &mut SillyBorgApp, ctx: &egui::Context) {
    while let Ok(action) = app.tray_rx.try_recv() {
        match action {
            TrayAction::Show => app.show_window(ctx),
            TrayAction::OpenFolder => app.open_output_dir(),
            TrayAction::Quit => {
                app.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            TrayAction::Update => app.restart_for_update(ctx),
            TrayAction::SaveClip => {} // handled on the tray thread
        }
    }
}

impl eframe::App for SillyBorgApp {
    /// Runs every pass, also while the window is hidden in the tray (then `ui` does not).
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        handle_tray(self, ctx);
        if self.show_requested.swap(false, Ordering::SeqCst) {
            self.show_window(ctx);
        }
        if self.hide_frames > 0 {
            self.hide_frames -= 1;
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.saved.close_to_tray {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_window(ctx);
        }

        self.library.update();
        if let Some(result) = self.library.trim_result.take() {
            match result {
                Ok(path) => {
                    self.editor = None;
                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    self.notify(ctx, Tone::Success, format!("Leikattu kopio tallennettu: {}", name));
                }
                Err(e) => self.notify(ctx, Tone::Danger, format!("Leikkaus epäonnistui: {}", e)),
            }
        }

        let update_state = self.updater.state();
        if update_state != self.last_update_state {
            if let UpdateState::Installed(m) = &update_state {
                // The window may be hidden in the tray for weeks: say so there, and offer the restart in the menu.
                if let Some(tray) = &self.tray {
                    tray.set_update(Some(&m.version));
                }
                if self.hidden {
                    crate::overlay::show(&format!("Päivitys {} asennettu", m.version), "Käynnistä uudelleen ilmaisinalueen valikosta", true);
                }
            }
            match &update_state {
                UpdateState::Installed(m) => self.notify(ctx, Tone::Success, format!("Päivitys {} asennettu. Se otetaan käyttöön seuraavalla käynnistyksellä.", m.version)),
                UpdateState::Available(m) => self.notify(ctx, Tone::Info, format!("Uusi versio {} saatavilla. Katso Sovellus-sivu.", m.version)),
                _ => {}
            }
            self.last_update_state = update_state;
        }

        let state = self.shared.status.lock().state.clone();
        if state != self.last_state {
            match &state {
                RunState::Failed(e) => self.notify(ctx, Tone::Danger, format!("Puskurin käynnistys epäonnistui: {}", e)),
                RunState::Running if self.announce_start => self.notify(ctx, Tone::Success, "Puskuri käynnissä"),
                _ => {}
            }
            if state != RunState::Starting {
                self.announce_start = false;
            }
            self.last_state = state.clone();
        }

        // Only tick while something on screen changes by itself; input repaints on its own.
        let busy = self.shared.saving.load(Ordering::SeqCst) || self.library.trimming || state == RunState::Starting;
        if !self.hidden && (self.page == Page::Overview || busy) {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        egui::Panel::left("sidebar")
            .exact_size(tk::SIDEBAR_W)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(tk::SIDEBAR_BG)
                    .stroke(Stroke::new(1.0, tk::LINE))
                    .inner_margin(Margin::symmetric(0, 20)),
            )
            .show(ui, |ui| self.sidebar(ui));

        egui::Panel::top("topbar")
            .exact_size(tk::TOPBAR_H)
            .resizable(false)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(tk::BG)
                    .stroke(Stroke::new(1.0, tk::LINE))
                    .inner_margin(Margin::symmetric(tk::PAGE_PAD, 0)),
            )
            .show(ui, |ui| self.topbar(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(tk::BG))
            .show(ui, |ui| {
                tk::ambient_glow(ui.painter(), ui.max_rect().left_top(), 520.0);
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(Margin { left: tk::PAGE_PAD, right: tk::PAGE_PAD, top: tk::PAGE_PAD, bottom: 96 })
                        .show(ui, |ui| {
                            ui.set_max_width(ui.available_width().min(1180.0));
                            match self.page {
                                Page::Overview => self.overview(ui),
                                Page::Clips => self.clips(ui),
                                Page::Source => self.source(ui),
                                Page::Audio => self.audio(ui),
                                Page::Quality => self.quality(ui),
                                Page::Hotkey => self.hotkey(ui),
                                Page::Profiles => self.profiles(ui),
                                Page::App => self.app(ui),
                            }
                        });
                });
            });

        self.save_bar(&ctx);
        self.toast(&ctx);
    }
}
