//! The window: app state, the frame (sidebar, top bar), floating bits (save bar, toast).
//! Each page lives in `pages/`; widgets and tokens in `crate::tiksu`.

mod pages;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{self, pos2, vec2, Align, Align2, Color32, CornerRadius, Layout, Margin, Rect, Sense, Stroke};
use egui_phosphor::regular::{
    ARROWS_CLOCKWISE, ARROW_CIRCLE_UP, ARROW_LEFT, ARROW_SQUARE_OUT, BELL, BOOK_OPEN, CARET_DOWN, CARET_RIGHT, CHECK_CIRCLE, CPU,
    FILM_STRIP, FOLDER_OPEN, GAME_CONTROLLER, GAUGE, GEAR, INFO, KEYBOARD, LIFEBUOY, LIGHTBULB, LIST_CHECKS, MEMORY, MICROPHONE, MONITOR,
    PLAY, PLUS, PLUS_CIRCLE, POWER, QUESTION, SCISSORS, SLIDERS, SPEAKER_HIGH, SQUARES_FOUR, TIMER, TRASH, WARNING, WAVEFORM, X,
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

/// Quality presets: (name, description, FPS, Mbps at 1080p, max height; 0 = captured size).
const QUALITY_PRESETS: [(&str, &str, u32, u32, u32); 5] = [
    ("Heikko kone", "Hidas tietokone tai kannettava: pieni kuva, kevyt kuorma.", 30, 5, 720),
    ("Kevyt", "Pienet tiedostot, kevyt kuorma.", 30, 8, 0),
    ("Tasapaino", "Suositus useimpiin peleihin.", 60, 20, 0),
    ("Terävä", "Nopea liike pysyy tarkkana.", 60, 30, 0),
    ("Kilpapeli", "120 FPS nopeille näytöille.", 120, 45, 0),
];

/// Largest picture height to encode at; 0 keeps the captured size.
const PICTURE_HEIGHTS: [(u32, &str); 4] = [(0, "Alkuperäinen"), (1440, "1440p"), (1080, "1080p"), (720, "720p")];

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
fn buffer_ram_mb(cfg: &AppConfig, bitrate: u32) -> u64 {
    bitrate as u64 / 8 * (cfg.max_clip_secs() + cfg.segment_duration_secs) / 1_000_000
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


/// Where a page sits in the sidebar. `Footer` pages are pinned to the bottom.
#[derive(Clone, Copy, PartialEq)]
enum Group {
    Main,
    Record,
    Manage,
    Footer,
}

impl Group {
    /// Sidebar groups from top to bottom, with their small heading (none for the first).
    const SIDEBAR: [(Group, Option<&'static str>); 3] =
        [(Group::Main, None), (Group::Record, Some("Tallennus")), (Group::Manage, Some("Hallinta"))];
}

/// Every page of the app.
///
/// To add a page: (1) add a variant here, (2) add it to `ALL` and to `label`, `icon` and `group`,
/// (3) add a match arm in `render_page`, (4) write `pages/<name>.rs` with one `impl SillyBorgApp` method that
/// calls `tk::page_head` and `tk::content`, and list the file in `pages/mod.rs`. The compiler points out
/// every match you forgot. The sidebar and breadcrumb pick the page up from this table.
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Overview,
    Clips,
    Help,
    Source,
    Audio,
    Quality,
    Hotkey,
    Profiles,
    App,
}

impl Page {
    /// In sidebar order.
    const ALL: [Page; 9] =
        [Page::Overview, Page::Clips, Page::Help, Page::Source, Page::Audio, Page::Quality, Page::Hotkey, Page::Profiles, Page::App];

    fn label(self) -> &'static str {
        match self {
            Page::Overview => "Yleiskatsaus",
            Page::Clips => "Leikkeet",
            Page::Help => "Ohje",
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
            Page::Help => QUESTION,
            Page::Source => MONITOR,
            Page::Audio => SPEAKER_HIGH,
            Page::Quality => GAUGE,
            Page::Hotkey => KEYBOARD,
            Page::Profiles => GAME_CONTROLLER,
            Page::App => GEAR,
        }
    }
    fn group(self) -> Group {
        match self {
            Page::Overview | Page::Clips | Page::Help => Group::Main,
            Page::Source | Page::Audio | Page::Quality => Group::Record,
            Page::Hotkey | Page::Profiles => Group::Manage,
            Page::App => Group::Footer,
        }
    }
}

struct Toast {
    text: String,
    tone: Tone,
    shown_at: f64,
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
    /// Hardware H.264 encoder Windows offers; `None` means video is encoded on the CPU.
    hw_encoder: Option<String>,
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
            hw_encoder: win::hardware_h264_encoder(),
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
            editor: start_editor(),
            confirm_delete: None,
            quitting: false,
            hidden: start_hidden,
            told_about_tray: start_hidden,
            hide_frames: if start_hidden { 3 } else { 0 },
            last_state: RunState::Starting,
            announce_start: false,
            page: start_page(),
            saved: config.clone(),
            config,
            shared,
        }
    }

    /// Size of the picture being captured (or, before capture starts, of the selected monitor).
    fn capture_size(&self) -> (u32, u32) {
        let size = self.shared.status.lock().capture_size;
        if size.0 > 0 {
            return size;
        }
        self.monitors
            .iter()
            .find(|m| m.index == self.config.monitor_index)
            .or(self.monitors.first())
            .map_or((1920, 1080), |m| (m.width, m.height))
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
    // Frame: sidebar, top bar, page dispatch
    // ------------------------------------------------------------------

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let problems = self.problems().len();
        let clips = self.library.clips.len();
        let count_for = |page: Page| match page {
            Page::Overview if problems > 0 => Some((Tone::Warning, problems.to_string())),
            Page::Clips if clips > 0 => Some((Tone::Neutral, clips.to_string())),
            Page::App => Some((Tone::Neutral, format!("v{}", update::CURRENT))),
            _ => None,
        };

        // Brand row, as tall as the top bar so the two lines meet.
        let (brand, _) = ui.allocate_exact_size(vec2(ui.available_width(), tk::TOPBAR_H), Sense::hover());
        {
            let p = ui.painter();
            let mark = Rect::from_center_size(brand.left_center() + vec2(24.0, 0.0), vec2(20.0, 20.0));
            p.rect_filled(mark, CornerRadius::same(tk::R_SM), tk::ACCENT);
            p.circle_stroke(mark.center(), 5.5, Stroke::new(1.5, Color32::WHITE));
            p.circle_filled(mark.center(), 2.0, Color32::WHITE);
            p.text(brand.left_center() + vec2(42.0, 0.0), Align2::LEFT_CENTER, "Tiksu Clipper", tk::semibold_font(13.0), tk::TEXT);
            p.hline(brand.x_range(), brand.bottom(), Stroke::new(1.0, tk::LINE));
        }

        let nav = |app: &mut Self, ui: &mut egui::Ui, page: Page| {
            let count = count_for(page);
            let count = count.as_ref().map(|(tone, text)| (*tone, text.as_str()));
            if tk::nav_item(ui, page.icon(), page.label(), app.page == page, count).clicked() {
                app.page = page;
            }
        };
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            egui::Frame::new().inner_margin(Margin::symmetric(8, 8)).show(ui, |ui| {
                for page in Page::ALL.into_iter().filter(|p| p.group() == Group::Footer) {
                    nav(self, ui, page);
                }
            });
            ui.add(egui::Separator::default().spacing(0.0));
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                egui::Frame::new().inner_margin(Margin::symmetric(8, 8)).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (group, title) in Group::SIDEBAR {
                        if let Some(title) = title {
                            ui.add_space(14.0);
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                tk::section_label(ui, title);
                            });
                            ui.add_space(2.0);
                        }
                        for page in Page::ALL.into_iter().filter(|p| p.group() == group) {
                            nav(self, ui, page);
                        }
                    }
                });
            });
        });
    }

    fn topbar(&mut self, ui: &mut egui::Ui) {
        let state = self.shared.status.lock().state.clone();
        let enabled = self.shared.is_enabled();
        ui.horizontal_centered(|ui| {
            ui.label(tk::hint("Tiksu Clipper"));
            ui.label(egui::RichText::new(CARET_RIGHT).size(12.0).color(tk::SUBTLE));
            ui.label(tk::body(self.page.label()));

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

    fn render_page(&mut self, ui: &mut egui::Ui) {
        match self.page {
            Page::Overview => self.overview(ui),
            Page::Clips => self.clips(ui),
            Page::Help => self.help(ui),
            Page::Source => self.source(ui),
            Page::Audio => self.audio(ui),
            Page::Quality => self.quality(ui),
            Page::Hotkey => self.hotkey(ui),
            Page::Profiles => self.profiles(ui),
            Page::App => self.app(ui),
        }
    }

    // ------------------------------------------------------------------
    // Floating: save bar, toast
    // ------------------------------------------------------------------

    fn save_bar(&mut self, ctx: &egui::Context) {
        if self.config == self.saved {
            return;
        }
        egui::Area::new(egui::Id::new("save_bar"))
            .anchor(Align2::CENTER_BOTTOM, vec2(tk::SIDEBAR_W / 2.0, -20.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(tk::SURFACE)
                    .stroke(Stroke::new(1.0, tk::LINE_STRONG))
                    .corner_radius(CornerRadius::same(tk::R_LG))
                    .inner_margin(Margin { left: 16, right: 8, top: 8, bottom: 8 })
                    .shadow(ui.visuals().popup_shadow)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            tk::dot(ui, tk::WARN);
                            ui.label(tk::body("Tallentamattomia muutoksia"));
                            ui.add_space(12.0);
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

    /// Top right under the top bar, 3px coloured left edge. Disappears after 5 s, errors stay.
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
            .anchor(Align2::RIGHT_TOP, vec2(-16.0, tk::TOPBAR_H + 12.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let resp = egui::Frame::new()
                    .fill(tk::SURFACE)
                    .stroke(Stroke::new(1.0, tk::LINE_STRONG))
                    .corner_radius(CornerRadius::same(tk::R_MD))
                    .inner_margin(Margin { left: 17, right: 8, top: 10, bottom: 10 })
                    .shadow(ui.visuals().popup_shadow)
                    .show(ui, |ui| {
                        ui.set_max_width(380.0);
                        ui.horizontal(|ui| {
                            ui.add(egui::Label::new(tk::body(&toast.text)).wrap());
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
            p.rect_filled(rect, CornerRadius::same(tk::R_SM), tk::SURFACE_2);
            p.text(rect.center(), Align2::CENTER_CENTER, FILM_STRIP, egui::FontId::proportional(20.0), tk::LINE_STRONG);
        }
    }
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
        // Dark title bar once the window exists, so the whole window reads as one surface.
        win::style_titlebar(tk::BG, tk::TEXT, tk::LINE);

        egui::Panel::left("sidebar")
            .exact_size(tk::SIDEBAR_W)
            .resizable(false)
            .frame(egui::Frame::new().fill(tk::SURFACE))
            .show(ui, |ui| self.sidebar(ui));

        egui::Panel::top("topbar")
            .exact_size(tk::TOPBAR_H)
            .resizable(false)
            .frame(egui::Frame::new().fill(tk::BG).inner_margin(Margin::symmetric(tk::PAGE_PAD, 0)))
            .show(ui, |ui| self.topbar(ui));

        egui::CentralPanel::default().frame(egui::Frame::new().fill(tk::BG)).show(ui, |ui| {
            ui.push_id(self.page.label(), |ui| self.render_page(ui));
        });

        self.save_bar(&ctx);
        self.toast(&ctx);
    }
}

/// Debug builds can open on a given page (`TIKSU_PAGE=Quality`), for checking pages without clicking.
fn start_page() -> Page {
    if cfg!(debug_assertions)
        && let Ok(name) = std::env::var("TIKSU_PAGE")
        && let Some(page) = Page::ALL.into_iter().find(|p| format!("{:?}", p.label()).trim_matches('"').eq_ignore_ascii_case(&name))
    {
        return page;
    }
    Page::Overview
}

/// Debug builds can open the trim editor on a file (`TIKSU_EDIT=path`).
fn start_editor() -> Option<Editor> {
    let path = std::env::var("TIKSU_EDIT").ok().filter(|_| cfg!(debug_assertions))?;
    Some(Editor { path: path.into(), start: 0.0, end: None, preview: 0.0 })
}
