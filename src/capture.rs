//! Replay buffer. A supervisor thread owns the capture session and audio recorder; the UI only
//! talks to it through [`Shared`], so starting, restarting and saving never block the window.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Local;
use parking_lot::Mutex;
use windows::core::Interface;
use windows::Storage::Streams::{DataReader, InMemoryRandomAccessStream};
use windows_capture::{
    capture::{CaptureControl, Context, GraphicsCaptureApiHandler},
    encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
        VideoSettingsSubType,
    },
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        GraphicsCaptureItemType, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
    window::Window,
};

use crate::audio::{now_hns, AudioRecorder, AudioTap, Bus, Track, HNS_PER_SEC};
use crate::buffer::{RingBufferManager, SegmentData, SegmentInfo};
use crate::config::{AppConfig, CaptureTargetType, DesktopAudioMode};
use crate::devices::find_window;
use crate::muxer::{concat_segments, ffmpeg_available};
use crate::notification::play_chime;

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type SessionControl = CaptureControl<ReplayCaptureSession, BoxError>;

#[derive(Debug, Clone, PartialEq)]
pub enum RunState {
    Stopped,
    Starting,
    Running,
    Failed(String),
}

/// What the supervisor reports to the UI.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub state: RunState,
    pub audio_warnings: Vec<String>,
    /// Exe of the game profile in use.
    pub active_profile: Option<String>,
    /// What is being captured, in words.
    pub target: String,
    /// The chosen window wasn't found, so the primary monitor is captured instead.
    pub fallback: bool,
    pub ffmpeg_ok: bool,
}

#[derive(Debug, Clone)]
pub struct LastClip {
    pub result: Result<PathBuf, String>,
}

/// State shared between the UI, hotkeys, tray and the supervisor.
pub struct Shared {
    /// Settings saved by the user. The supervisor applies them (plus any game profile).
    desired: Mutex<AppConfig>,
    /// Settings the buffer actually runs with right now.
    live: Mutex<AppConfig>,
    enabled: AtomicBool,
    quit: AtomicBool,
    wake: Sender<()>,
    trigger_tx: Sender<u64>,
    trigger_rx: Mutex<Receiver<u64>>,
    pub status: Mutex<Status>,
    pub ring: RingBufferManager,
    pub saving: AtomicBool,
    /// Set when a capture worker thread panicked; the supervisor then restarts the session.
    worker_failed: AtomicBool,
    pub last_clip: Mutex<Option<LastClip>>,
    /// Bumped after every successful save, so the clip library knows to rescan.
    pub clips_saved: AtomicU64,
    repaint: OnceLock<eframe::egui::Context>,
}

impl Shared {
    pub fn config(&self) -> AppConfig {
        self.desired.lock().clone()
    }

    pub fn live_config(&self) -> AppConfig {
        self.live.lock().clone()
    }

    pub fn set_config(&self, config: AppConfig) {
        *self.desired.lock() = config;
        let _ = self.wake.send(());
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::SeqCst);
        let _ = self.wake.send(());
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Downloads ffmpeg in the background if it is missing (see `ffmpeg.rs`).
    pub fn ensure_ffmpeg(self: &Arc<Self>) {
        let me = self.clone();
        crate::ffmpeg::install(move |ok| {
            me.status.lock().ffmpeg_ok = ok;
            me.poke();
        });
    }

    /// `secs == 0` saves the main clip length.
    pub fn save_clip(&self, secs: u64) {
        let _ = self.trigger_tx.send(secs);
    }

    pub fn trigger_sender(&self) -> Sender<u64> {
        self.trigger_tx.clone()
    }

    pub fn set_repaint_context(&self, ctx: eframe::egui::Context) {
        let _ = self.repaint.set(ctx);
    }

    fn poke(&self) {
        if let Some(ctx) = self.repaint.get() {
            ctx.request_repaint();
        }
    }

    /// Runs a worker thread body; a panic is logged and makes the supervisor restart the session,
    /// instead of leaving the UI saying "running" while nothing records.
    fn guarded(&self, what: &str, f: impl FnOnce()) {
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).is_err() {
            println!("{} kaatui, puskuri käynnistetään uudelleen", what);
            self.worker_failed.store(true, Ordering::SeqCst);
        }
    }

    /// Updates the status; wakes the UI only if something actually changed.
    fn set_status(&self, f: impl FnOnce(&mut Status)) {
        let changed = {
            let mut status = self.status.lock();
            let before = status.clone();
            f(&mut status);
            *status != before
        };
        if changed {
            self.poke();
        }
    }
}

/// Starts the supervisor. Call [`shutdown`] before exit so temp files are removed.
pub fn spawn(config: AppConfig) -> (Arc<Shared>, JoinHandle<()>) {
    let (wake, wake_rx) = channel();
    let (trigger_tx, trigger_rx) = channel();
    let shared = Arc::new(Shared {
        ring: RingBufferManager::new(config.max_clip_secs() as f64),
        live: Mutex::new(config.clone()),
        desired: Mutex::new(config),
        enabled: AtomicBool::new(true),
        quit: AtomicBool::new(false),
        wake,
        trigger_tx,
        trigger_rx: Mutex::new(trigger_rx),
        status: Mutex::new(Status {
            state: RunState::Starting,
            audio_warnings: Vec::new(),
            active_profile: None,
            target: String::new(),
            fallback: false,
            ffmpeg_ok: true,
        }),
        saving: AtomicBool::new(false),
        worker_failed: AtomicBool::new(false),
        last_clip: Mutex::new(None),
        clips_saved: AtomicU64::new(0),
        repaint: OnceLock::new(),
    });
    let handle = {
        let shared = shared.clone();
        thread::spawn(move || Supervisor::new(shared).run(wake_rx))
    };
    (shared, handle)
}

pub fn shutdown(shared: &Shared, handle: JoinHandle<()>) {
    shared.quit.store(true, Ordering::SeqCst);
    let _ = shared.wake.send(());
    let _ = handle.join();
}

// ----------------------------------------------------------------------------
// Supervisor
// ----------------------------------------------------------------------------

struct Supervisor {
    shared: Arc<Shared>,
    tap: AudioTap,
    audio: Option<AudioRecorder>,
    /// Config the audio sources were opened with.
    audio_cfg: Option<AppConfig>,
    session: Option<SessionControl>,
    /// Config the capture session was started with.
    running: Option<AppConfig>,
    profile: Option<String>,
    /// Last failed start, retried after a pause instead of every tick.
    failed: Option<(AppConfig, Instant)>,
    temp_dir: PathBuf,
    session_count: u32,
    tick: u64,
}

impl Supervisor {
    fn new(shared: Arc<Shared>) -> Self {
        // Buffer folders of processes that no longer run are leftovers from a crash.
        let tmp = std::env::temp_dir();
        let pids: Vec<u32> = crate::win::processes().iter().map(|(pid, _, _)| *pid).collect();
        if let Ok(entries) = std::fs::read_dir(&tmp) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let pid = name.strip_prefix("silly_borg_buf_").and_then(|p| p.parse::<u32>().ok());
                if pid.is_some_and(|pid| !pids.contains(&pid)) {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        let temp_dir = tmp.join(format!("silly_borg_buf_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let ffmpeg_ok = ffmpeg_available();
        shared.status.lock().ffmpeg_ok = ffmpeg_ok;
        if !ffmpeg_ok {
            // The replay buffer already runs; clips can be saved once the download is done.
            println!("ffmpeg puuttuu, ladataan");
            shared.ensure_ffmpeg();
        }

        Self {
            shared,
            tap: AudioTap::new(0),
            audio: None,
            audio_cfg: None,
            session: None,
            running: None,
            profile: None,
            failed: None,
            temp_dir,
            session_count: 0,
            tick: 0,
        }
    }

    fn run(mut self, wake_rx: Receiver<()>) {
        while !self.shared.quit.load(Ordering::SeqCst) {
            let shared = self.shared.clone();
            shared.guarded("Valvoja", || self.step());
            if shared.worker_failed.swap(false, Ordering::SeqCst) {
                // Drop everything so the next tick starts from scratch; the retry pause stops a crash loop.
                self.stop_session();
                self.audio = None;
                self.audio_cfg = None;
                self.shared.ring.clear_and_delete_all();
                let effective = self.shared.live_config();
                self.failed = Some((effective, Instant::now()));
                self.shared.set_status(|s| s.state = RunState::Failed("Sisäinen virhe, puskuri käynnistyy uudelleen".to_string()));
            }
            match wake_rx.recv_timeout(Duration::from_millis(500)) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        self.stop_session();
        self.audio = None;
        self.shared.ring.clear_and_delete_all();
        let _ = std::fs::remove_dir_all(&self.temp_dir);
    }

    fn step(&mut self) {
        self.tick += 1;
        let desired = self.shared.config();

        if !self.shared.is_enabled() {
            if self.session.is_some() || self.audio.is_some() {
                println!("Puskuri pysäytetty");
                self.stop_session();
                self.audio = None;
                self.audio_cfg = None;
                self.shared.ring.clear_and_delete_all();
            }
            self.shared.set_status(|s| s.state = RunState::Stopped);
            self.drain_triggers();
            return;
        }

        // Game profiles: checked every 2 s, it walks the process list.
        if self.tick % 4 == 1 {
            let profile = detect_profile(&desired, self.profile.as_deref());
            if profile != self.profile {
                println!("Peliprofiili: {:?}", profile);
                self.profile = profile;
            }
        }
        let effective = match self.profile.as_deref().and_then(|exe| desired.profile_for(exe)) {
            Some(p) => desired.with_profile(p),
            None => desired.clone(),
        };

        if self.audio_cfg.as_ref().is_none_or(|a| a.audio_differs(&effective)) {
            self.start_audio(&effective);
        }
        let keep = effective.max_clip_secs() + 3 * effective.segment_duration_secs + 10;
        self.tap.set_keep_secs(keep);
        self.shared.ring.update_max_duration(effective.max_clip_secs() as f64);

        let session_dead = self.session.as_ref().is_none_or(|s| s.is_finished());
        let fallback = self.shared.status.lock().fallback;
        let window_back = fallback && self.tick % 4 == 1 && find_window(&effective).is_some();
        let needs_restart = session_dead
            || window_back
            || self.running.as_ref().is_none_or(|r| r.video_differs(&effective));

        if needs_restart {
            let recently_failed = self
                .failed
                .as_ref()
                .is_some_and(|(cfg, at)| *cfg == effective && at.elapsed() < Duration::from_secs(5));
            if !recently_failed {
                self.start_session(&effective);
            }
        }
        if self.session.is_none() {
            self.drain_triggers();
        }
        *self.shared.live.lock() = effective;
        let profile = self.profile.clone();
        self.shared.set_status(|s| s.active_profile = profile);
    }

    /// Saves can't happen without a session; tell the user instead of saving later by surprise.
    fn drain_triggers(&self) {
        if self.shared.trigger_rx.lock().try_iter().count() > 0 {
            let cfg = self.shared.config();
            notify(&cfg, false, "Puskuri ei ole käynnissä", "Käynnistä puskuri Tiksu Clipperistä");
        }
    }

    fn start_audio(&mut self, cfg: &AppConfig) {
        self.audio = None; // stops the old sources
        let rec = AudioRecorder::start(cfg, &self.tap);
        for w in &rec.warnings {
            println!("Ääni: {}", w);
        }
        let warnings = rec.warnings.clone();
        self.shared.set_status(|s| s.audio_warnings = warnings);
        self.audio = Some(rec);
        self.audio_cfg = Some(cfg.clone());
    }

    fn stop_session(&mut self) {
        if let Some(session) = self.session.take() {
            let _ = session.stop();
        }
        self.running = None;
    }

    fn start_session(&mut self, cfg: &AppConfig) {
        self.shared.set_status(|s| s.state = RunState::Starting);
        self.stop_session();
        // Old segments may have another resolution and can't be stream-copied together with new ones.
        self.shared.ring.clear_and_delete_all();
        let _ = std::fs::create_dir_all(&cfg.output_dir);

        match self.open_target(cfg) {
            Ok((control, target, fallback)) => {
                println!("Puskuri käynnissä: {}", target);
                self.session = Some(control);
                self.running = Some(cfg.clone());
                self.failed = None;
                self.shared.set_status(|s| {
                    s.state = RunState::Running;
                    s.target = target;
                    s.fallback = fallback;
                });
            }
            Err(e) => {
                println!("Puskurin käynnistys epäonnistui: {}", e);
                self.failed = Some((cfg.clone(), Instant::now()));
                self.shared.set_status(|s| s.state = RunState::Failed(e));
            }
        }
    }

    fn open_target(&mut self, cfg: &AppConfig) -> Result<(SessionControl, String, bool), String> {
        if let Some(window) = find_window(cfg) {
            let w = window.width().unwrap_or(1280).max(128) as u32;
            let h = window.height().unwrap_or(720).max(128) as u32;
            let label = window.process_name().unwrap_or_else(|_| cfg.window_title.clone());
            let control = self
                .start_capture(window, w, h, cfg)
                .map_err(|e| format!("Ikkunan nauhoituksen käynnistys epäonnistui: {}", e))?;
            return Ok((control, label, false));
        }
        // Window not found: fall back to the primary monitor so the replay buffer always runs.
        let fallback = cfg.capture_source_type == CaptureTargetType::Window;
        let (monitor, index) = match cfg.capture_source_type {
            CaptureTargetType::Monitor => (
                Monitor::from_index(cfg.monitor_index).or_else(|_| Monitor::primary()),
                cfg.monitor_index,
            ),
            CaptureTargetType::Window => (Monitor::primary(), 1),
        };
        let monitor = monitor.map_err(|e| format!("Näyttöä ei voitu avata: {:?}", e))?;
        let w = monitor.width().unwrap_or(1920);
        let h = monitor.height().unwrap_or(1080);
        let control = self
            .start_capture(monitor, w, h, cfg)
            .map_err(|e| format!("Replay-istunnon käynnistys epäonnistui: {}", e))?;
        Ok((control, format!("Näyttö {}", index), fallback))
    }

    fn start_capture<T: TryInto<GraphicsCaptureItemType> + Send + 'static>(
        &mut self,
        item: T,
        width: u32,
        height: u32,
        cfg: &AppConfig,
    ) -> Result<SessionControl, String> {
        // Each session gets its own folder so file names never collide with a previous session's.
        self.session_count += 1;
        let session_dir = self.temp_dir.join(format!("s{}", self.session_count));
        let _ = std::fs::create_dir_all(&session_dir);

        let flags = CaptureFlags {
            width: (width / 2) * 2,
            height: (height / 2) * 2,
            config: cfg.clone(),
            temp_dir: session_dir,
            shared: self.shared.clone(),
            tap: self.tap.clone(),
        };
        let interval_ms = (1000.0 / cfg.frame_rate as f64).round() as u64;
        let settings = Settings::new(
            item,
            CursorCaptureSettings::Default,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Custom(Duration::from_millis(interval_ms)),
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            flags,
        );
        ReplayCaptureSession::start_free_threaded(settings).map_err(|e| format!("{:?}", e))
    }
}

/// The profile to use: the foreground game's, else the current one while its game still runs.
fn detect_profile(cfg: &AppConfig, current: Option<&str>) -> Option<String> {
    if !cfg.auto_profiles || cfg.profiles.is_empty() {
        return None;
    }
    let foreground = Window::foreground().ok().and_then(|w| w.process_name().ok());
    if let Some(p) = foreground.and_then(|exe| cfg.profile_for(&exe)) {
        return Some(p.exe.clone());
    }
    let current = current.filter(|exe| cfg.profile_for(exe).is_some())?;
    crate::win::processes()
        .iter()
        .any(|(_, _, name)| name.eq_ignore_ascii_case(current))
        .then(|| current.to_string())
}

// ----------------------------------------------------------------------------
// Capture session
// ----------------------------------------------------------------------------

#[derive(Clone)]
pub struct CaptureFlags {
    width: u32,
    height: u32,
    config: AppConfig,
    temp_dir: PathBuf,
    shared: Arc<Shared>,
    tap: AudioTap,
}

/// Where the encoder writes.
enum Sink {
    Memory(InMemoryRandomAccessStream),
    File(PathBuf),
}

/// The segment currently being encoded. Video is encoded without audio; audio is muxed in at save time.
struct Segment {
    encoder: VideoEncoder,
    sink: Sink,
    start_hns: Option<i64>,
    frames: u32,
}

impl Segment {
    fn new(flags: &CaptureFlags, index: u64) -> Result<Self, BoxError> {
        let video = VideoSettingsBuilder::new(flags.width, flags.height)
            .sub_type(VideoSettingsSubType::H264)
            .bitrate(flags.config.bitrate)
            .frame_rate(flags.config.frame_rate);
        let audio = AudioSettingsBuilder::new().disabled(true);
        let (encoder, sink) = if flags.config.buffer_in_memory {
            let stream = InMemoryRandomAccessStream::new()?;
            let enc = VideoEncoder::new_from_stream(video, audio, ContainerSettingsBuilder::new(), stream.cast()?)?;
            (enc, Sink::Memory(stream))
        } else {
            let path = flags.temp_dir.join(format!("seg_{}.mp4", index));
            (VideoEncoder::new(video, audio, ContainerSettingsBuilder::new(), &path)?, Sink::File(path))
        };
        Ok(Self { encoder, sink, start_hns: None, frames: 0 })
    }

    /// Finishes encoding; `None` if the segment got no frames or the encoder failed.
    fn finish(self, end_hns: i64) -> Option<SegmentInfo> {
        let Segment { encoder, sink, start_hns, frames } = self;
        let ok = encoder.finish().is_ok() && frames > 0;
        let data = match sink {
            Sink::Memory(stream) if ok => SegmentData::Memory(read_stream(&stream).ok()?.into()),
            Sink::File(path) if ok => SegmentData::File(path),
            Sink::File(path) => {
                let _ = std::fs::remove_file(path);
                return None;
            }
            Sink::Memory(_) => return None,
        };
        Some(SegmentInfo { data, start_hns: start_hns?, end_hns })
    }
}

fn read_stream(stream: &InMemoryRandomAccessStream) -> windows::core::Result<Vec<u8>> {
    let size = stream.Size()? as u32;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    reader.LoadAsync(size)?.join()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf)?;
    Ok(buf)
}

/// Receives frames only. Rotation and clip saving run on a timer thread, because a window
/// capture delivers no frames while the window is static and a save must not wait for one.
pub struct ReplayCaptureSession {
    current: Arc<Mutex<Segment>>,
    alive: Arc<AtomicBool>,
}

impl GraphicsCaptureApiHandler for ReplayCaptureSession {
    type Flags = CaptureFlags;
    type Error = BoxError;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let flags = ctx.flags;
        let current = Arc::new(Mutex::new(Segment::new(&flags, 0)?));
        let alive = Arc::new(AtomicBool::new(true));

        // Finalizer: finishes rotated encoders in order and adds them to the ring buffer.
        let (finish_tx, finish_rx) = channel::<(Segment, i64, Option<Sender<()>>)>();
        {
            let ring = flags.shared.ring.clone();
            // Segments finishing after the buffer was cleared for a new session are dropped by generation.
            let generation = ring.generation();
            let shared = flags.shared.clone();
            thread::spawn(move || {
                shared.guarded("Segmenttien viimeistely", || {
                    while let Ok((seg, end_hns, done)) = finish_rx.recv() {
                        if let Some(info) = seg.finish(end_hns) {
                            ring.add_segment(info, generation);
                        }
                        if let Some(done) = done {
                            let _ = done.send(());
                        }
                    }
                })
            });
        }

        // Rotation / save timer. The next encoder is created only when needed (not pre-warmed),
        // so a second hardware encoder session exists just for the moment of a rotation.
        {
            let current = current.clone();
            let alive = alive.clone();
            let shared = flags.shared.clone();
            thread::spawn(move || shared.guarded("Ajastin", move || {
                let seg_hns = flags.config.segment_duration_secs.max(1) as i64 * HNS_PER_SEC;
                let mut index = 0u64;
                let mut pending: Option<u64> = None;
                while alive.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_millis(20));
                    let requests: Vec<u64> = flags.shared.trigger_rx.lock().try_iter().collect();
                    for secs in requests {
                        // 0 = the main clip length; several presses at once save the longest asked for.
                        let secs = if secs == 0 { flags.shared.live_config().clip_duration_secs } else { secs };
                        pending = Some(pending.map_or(secs, |p: u64| p.max(secs)));
                        flags.shared.saving.store(true, Ordering::SeqCst);
                    }
                    let due = current.lock().start_hns.is_some_and(|start| now_hns() - start >= seg_hns);
                    if pending.is_none() && !due {
                        continue;
                    }

                    index += 1;
                    let next = match Segment::new(&flags, index) {
                        Ok(seg) => seg,
                        Err(e) => {
                            // Keep recording into the current segment and retry; a pending save stays pending.
                            println!("Enkooderin käynnistys epäonnistui: {}", e);
                            thread::sleep(Duration::from_millis(500));
                            continue;
                        }
                    };
                    let old = std::mem::replace(&mut *current.lock(), next);
                    let end_hns = now_hns();

                    let done = pending.take().map(|secs| {
                        let (done_tx, done_rx) = channel();
                        let flags = flags.clone();
                        thread::spawn(move || {
                            let shared = flags.shared.clone();
                            shared.guarded("Tallennus", || save_clip(done_rx, secs, &flags, index));
                            // A panic must not leave the "saving" spinner on forever.
                            shared.saving.store(false, Ordering::SeqCst);
                        });
                        done_tx
                    });
                    let _ = finish_tx.send((old, end_hns, done));
                }
            }));
        }

        Ok(Self { current, alive })
    }

    fn on_frame_arrived(&mut self, frame: &mut Frame<'_>, _control: InternalCaptureControl) -> Result<(), Self::Error> {
        let mut seg = self.current.lock();
        if seg.start_hns.is_none() {
            seg.start_hns = Some(frame.timestamp()?.Duration);
        }
        seg.encoder.send_frame(frame)?;
        seg.frames += 1;
        Ok(())
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        println!("Kuvattava ikkuna suljettiin");
        Ok(())
    }
}

impl Drop for ReplayCaptureSession {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
    }
}

// ----------------------------------------------------------------------------
// Saving
// ----------------------------------------------------------------------------

fn notify(cfg: &AppConfig, success: bool, title: &str, subtitle: &str) {
    if cfg.play_sound {
        play_chime(success);
    }
    if cfg.overlay {
        crate::overlay::show(title, subtitle, success);
    }
}

/// Waits for the just-rotated segment to be finalized, then writes the clip.
fn save_clip(done_rx: Receiver<()>, secs: u64, flags: &CaptureFlags, index: u64) {
    let _ = done_rx.recv_timeout(Duration::from_secs(15));
    let shared = &flags.shared;
    let cfg = shared.live_config();

    let prefix = shared
        .status
        .lock()
        .active_profile
        .as_deref()
        .and_then(|exe| Path::new(exe).file_stem().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "clip".to_string());
    let stamp = format!("{}_{}", prefix, Local::now().format("%Y-%m-%d_%H-%M-%S"));
    // Two saves within the same second must not overwrite each other (ffmpeg runs with -y).
    let out_path = (1..)
        .map(|n| cfg.output_dir.join(if n == 1 { format!("{}.mp4", stamp) } else { format!("{}_{}.mp4", stamp, n) }))
        .find(|p| !p.exists())
        .unwrap();
    let out_name = out_path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let work_dir = flags.temp_dir.join(format!("save_{}", index));

    let result = if shared.status.lock().ffmpeg_ok {
        write_clip(&shared.ring.last_segments(secs as f64), &flags.tap, &cfg, &work_dir, &out_path)
    } else {
        // The buffer keeps recording, so pressing the key again in a moment works.
        Err("ffmpeg latautuu vielä. Yritä hetken kuluttua uudelleen.".to_string())
    };
    let _ = std::fs::remove_dir_all(&work_dir);

    match &result {
        Ok(()) => {
            println!("Leike tallennettu: {} ({} s)", out_path.display(), secs);
            notify(&cfg, true, "Leike tallennettu", &format!("{} s · {}", secs, out_name));
            shared.clips_saved.fetch_add(1, Ordering::SeqCst);
        }
        Err(e) => {
            println!("Tallennus epäonnistui: {}", e);
            notify(&cfg, false, "Tallennus epäonnistui", e);
        }
    }
    *shared.last_clip.lock() = Some(LastClip { result: result.map(|_| out_path) });
    shared.saving.store(false, Ordering::SeqCst);
    shared.poke();
}

fn write_clip(segments: &[SegmentInfo], tap: &AudioTap, cfg: &AppConfig, work_dir: &Path, out: &Path) -> Result<(), String> {
    let (first, last) = match (segments.first(), segments.last()) {
        (Some(f), Some(l)) => (f.start_hns, l.end_hns),
        _ => return Err("Puskurissa ei ole vielä kuvaa tallennettavaksi".to_string()),
    };
    // Fail early with a readable message instead of a half-written clip. The final MP4 is about as
    // big as the video segments; memory segments and the WAVs also pass through the temp drive.
    let video_bytes: u64 = segments
        .iter()
        .map(|s| match &s.data {
            SegmentData::Memory(b) => b.len() as u64,
            SegmentData::File(p) => std::fs::metadata(p).map_or(0, |m| m.len()),
        })
        .sum();
    let wav_bytes = (last - first).max(0) as u64 / HNS_PER_SEC as u64 * 48_000 * 4 * 3;
    for (dir, need) in [(out.parent().unwrap_or(work_dir), video_bytes + 20_000_000), (work_dir, video_bytes + wav_bytes)] {
        if let Some(free) = crate::win::free_disk_bytes(dir).filter(|free| *free < need) {
            return Err(format!("Levyllä ei ole tarpeeksi tilaa ({} Mt vapaana, {} Mt tarvitaan)", free / 1_000_000, need / 1_000_000));
        }
    }
    std::fs::create_dir_all(work_dir).map_err(|e| e.to_string())?;

    // ffmpeg reads files: write in-memory segments out (only now, at save time).
    let mut files = Vec::with_capacity(segments.len());
    for (i, seg) in segments.iter().enumerate() {
        let data = match &seg.data {
            SegmentData::Memory(bytes) => {
                let path = work_dir.join(format!("seg_{}.mp4", i));
                std::fs::write(&path, bytes).map_err(|e| format!("Väliaikaistiedoston kirjoitus epäonnistui: {}", e))?;
                SegmentData::File(path)
            }
            file => file.clone(),
        };
        files.push(SegmentInfo { data, ..seg.clone() });
    }

    let desktop = cfg.desktop_audio != DesktopAudioMode::Off;
    let mic = cfg.record_microphone;
    let mut tracks: Vec<(Track, &str)> = Vec::new();
    if desktop || mic {
        tracks.push((Track::Mix, "Kaikki äänet"));
    }
    if cfg.separate_audio_tracks && desktop && mic {
        tracks.push((Track::Only(Bus::Desktop), "Tietokone"));
        tracks.push((Track::Only(Bus::Mic), "Mikrofoni"));
    }
    let mut wavs = Vec::new();
    for (i, (track, title)) in tracks.into_iter().enumerate() {
        let wav = work_dir.join(format!("audio_{}.wav", i));
        tap.write_wav(first, last, track, &wav).map_err(|e| format!("Äänen kirjoitus epäonnistui: {}", e))?;
        wavs.push((wav, title));
    }
    concat_segments(&files, &wavs, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manual end-to-end: records the primary monitor + default audio, saves a clip mid-segment.
    /// `cargo test end_to_end -- --ignored`
    #[test]
    #[ignore]
    #[allow(clippy::field_reassign_with_default)]
    fn end_to_end_clip_has_video_and_audio_tracks() {
        let out_dir = std::env::temp_dir().join("silly_borg_e2e");
        let _ = std::fs::remove_dir_all(&out_dir);
        let mut cfg = AppConfig::default();
        cfg.output_dir = out_dir.clone();
        cfg.capture_source_type = CaptureTargetType::Monitor;
        cfg.segment_duration_secs = 3;
        cfg.record_microphone = true;
        cfg.separate_audio_tracks = true;
        cfg.play_sound = false;
        cfg.overlay = false;
        cfg.profiles.clear();

        let (shared, handle) = spawn(cfg);
        thread::sleep(Duration::from_secs(8));
        assert_eq!(shared.status.lock().state, RunState::Running);
        assert!(shared.status.lock().audio_warnings.is_empty(), "{:?}", shared.status.lock().audio_warnings);
        assert!(shared.ring.stats().2 > 0, "segments should be in memory");
        shared.save_clip(5);
        thread::sleep(Duration::from_millis(200));
        while shared.saving.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(100));
        }
        let saved = shared.last_clip.lock().clone().unwrap().result.unwrap();
        shutdown(&shared, handle);

        let probe = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "stream=codec_type:format=duration", "-of", "csv=p=0"])
            .arg(&saved)
            .output()
            .unwrap();
        let info = String::from_utf8_lossy(&probe.stdout).to_string();
        std::fs::write(std::env::temp_dir().join("silly_borg_e2e.txt"), &info).unwrap();
        assert_eq!(info.matches("audio").count(), 3, "{}", info);
        assert!(info.contains("video"), "{}", info);
    }
}
