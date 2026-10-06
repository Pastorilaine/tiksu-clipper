//! Audio capture: output-device loopback, single-application loopback and microphone,
//! mixed into one ring of 48 kHz stereo i16 that is indexed by QPC wall-clock time.
//! Video segments use the same clock, so any time range can be cut out in sync at save time.

use std::collections::VecDeque;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;
use wasapi::{
    AudioCaptureClient, AudioClient, DeviceEnumerator, Direction, Handle, SampleType, StreamMode,
    WaveFormat,
};

use crate::config::{AppConfig, DesktopAudioMode};

pub const DEFAULT_DEVICE: &str = "Oletuslaite (Järjestelmän oletus)";
pub const HNS_PER_SEC: i64 = 10_000_000;
const SAMPLE_RATE: i64 = 48_000;
const CHANNELS: usize = 2;

/// Current QPC time in 100 ns units: the same clock as WASAPI packet and WGC frame timestamps.
pub fn now_hns() -> i64 {
    let (mut count, mut freq) = (0i64, 1i64);
    unsafe {
        windows_sys::Win32::System::Performance::QueryPerformanceCounter(&mut count);
        windows_sys::Win32::System::Performance::QueryPerformanceFrequency(&mut freq);
    }
    (count as i128 * HNS_PER_SEC as i128 / freq as i128) as i64
}

fn frame_at(hns: i64) -> i64 {
    (hns as i128 * SAMPLE_RATE as i128 / HNS_PER_SEC as i128) as i64
}

/// Interleaved stereo samples starting at absolute frame `first_frame`.
struct Ring {
    first_frame: i64,
    samples: VecDeque<i16>,
    max_frames: usize,
}

impl Ring {
    /// Mixes `data` into the ring at absolute frame `at`; drops audio older than `max_frames`.
    fn mix(&mut self, at: i64, data: &[f32]) {
        if self.samples.is_empty() || at - self.first_frame > self.max_frames as i64 {
            self.samples.clear();
            self.first_frame = at;
        }
        let mut offset = (at - self.first_frame) * CHANNELS as i64;
        let mut data = data;
        if offset < 0 {
            data = &data[(-offset as usize).min(data.len())..];
            offset = 0;
        }
        let offset = offset as usize;
        if self.samples.len() < offset + data.len() {
            self.samples.resize(offset + data.len(), 0);
        }
        for (slot, s) in self.samples.range_mut(offset..).zip(data) {
            // ponytail: saturating i16 mix, half the RAM of an f32 accumulator; clips only when sources are already near full scale
            *slot = slot.saturating_add((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
        }
        let excess = self.samples.len().saturating_sub(self.max_frames * CHANNELS);
        if excess > 0 {
            self.samples.drain(..excess);
            self.first_frame += (excess / CHANNELS) as i64;
        }
    }
}

/// Read handle to the recorded audio: one ring per bus, mixed (or not) at save time.
#[derive(Clone)]
pub struct AudioTap(Arc<[Mutex<Ring>; 2]>);

/// Where a source's audio goes.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Bus {
    Desktop = 0,
    Mic = 1,
}

/// Which audio a WAV contains.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Track {
    Mix,
    Only(Bus),
}

impl AudioTap {
    pub fn new(keep_secs: u64) -> Self {
        let ring = || Mutex::new(Ring { first_frame: 0, samples: VecDeque::new(), max_frames: 0 });
        let tap = Self(Arc::new([ring(), ring()]));
        tap.set_keep_secs(keep_secs);
        tap
    }

    /// How much audio history to keep; growing takes effect as new audio arrives.
    pub fn set_keep_secs(&self, secs: u64) {
        for ring in self.0.iter() {
            ring.lock().max_frames = secs as usize * SAMPLE_RATE as usize;
        }
    }

    fn mix(&self, bus: Bus, at: i64, data: &[f32]) {
        self.0[bus as usize].lock().mix(at, data);
    }

    /// Writes the audio between two QPC times as 16-bit stereo WAV. Missing audio becomes silence.
    pub fn write_wav(&self, from_hns: i64, to_hns: i64, track: Track, path: &Path) -> std::io::Result<()> {
        let start = frame_at(from_hns);
        let frames = (frame_at(to_hns) - start).max(0) as usize;
        let mut pcm = vec![0i16; frames * CHANNELS];
        let buses: &[Bus] = match track {
            Track::Mix => &[Bus::Desktop, Bus::Mic],
            Track::Only(Bus::Desktop) => &[Bus::Desktop],
            Track::Only(Bus::Mic) => &[Bus::Mic],
        };
        for bus in buses {
            let ring = self.0[*bus as usize].lock();
            let skip = (start - ring.first_frame) * CHANNELS as i64;
            let (dst, src) = if skip >= 0 { (0, skip as usize) } else { ((-skip) as usize, 0) };
            for (out, s) in pcm.iter_mut().skip(dst).zip(ring.samples.range(src.min(ring.samples.len())..)) {
                *out = out.saturating_add(*s);
            }
        }

        let data_len = (pcm.len() * 2) as u32;
        let byte_rate = SAMPLE_RATE as u32 * CHANNELS as u32 * 2;
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
        f.write_all(b"RIFF")?;
        f.write_all(&(36 + data_len).to_le_bytes())?;
        f.write_all(b"WAVEfmt ")?;
        f.write_all(&16u32.to_le_bytes())?;
        f.write_all(&1u16.to_le_bytes())?; // PCM
        f.write_all(&(CHANNELS as u16).to_le_bytes())?;
        f.write_all(&(SAMPLE_RATE as u32).to_le_bytes())?;
        f.write_all(&byte_rate.to_le_bytes())?;
        f.write_all(&(CHANNELS as u16 * 2).to_le_bytes())?;
        f.write_all(&16u16.to_le_bytes())?;
        f.write_all(b"data")?;
        f.write_all(&data_len.to_le_bytes())?;
        for s in pcm {
            f.write_all(&s.to_le_bytes())?;
        }
        f.flush()
    }
}

enum Source {
    Device(Direction, String),
    App(String),
}

impl Source {
    fn label(&self) -> String {
        match self {
            Source::Device(Direction::Render, name) => format!("Äänilaite \"{}\"", name),
            Source::Device(Direction::Capture, name) => format!("Mikrofoni \"{}\"", name),
            Source::App(name) => format!("Sovellus \"{}\"", name),
        }
    }

    fn bus(&self) -> Bus {
        match self {
            Source::Device(Direction::Capture, _) => Bus::Mic,
            _ => Bus::Desktop,
        }
    }
}

/// Runs one capture thread per selected audio source. Stops when dropped.
pub struct AudioRecorder {
    pub warnings: Vec<String>,
    running: Arc<AtomicBool>,
}

impl AudioRecorder {
    /// Starts every audio source selected in `config`, writing into `tap`.
    pub fn start(config: &AppConfig, tap: &AudioTap) -> Self {
        let mut sources = Vec::new();
        match config.desktop_audio {
            DesktopAudioMode::Off => {}
            DesktopAudioMode::Device => {
                sources.push(Source::Device(Direction::Render, config.output_device.clone()))
            }
            DesktopAudioMode::Application => {
                sources.push(Source::App(config.app_audio_process.clone()))
            }
        }
        if config.record_microphone {
            sources.push(Source::Device(Direction::Capture, config.microphone_device.clone()));
        }

        let running = Arc::new(AtomicBool::new(true));
        let warnings = sources
            .into_iter()
            .filter_map(|src| spawn_source(src, tap.clone(), running.clone()).err())
            .collect();
        Self { warnings, running }
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Spawns the capture thread and waits for the first open attempt. A failed source keeps
/// retrying in the background (e.g. the selected app is started later or a device is plugged in).
fn spawn_source(src: Source, tap: AudioTap, running: Arc<AtomicBool>) -> Result<(), String> {
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let label = src.label();
    thread::spawn(move || {
        let _ = wasapi::initialize_mta();
        let mut ready_tx = Some(ready_tx);
        while running.load(Ordering::SeqCst) {
            match open_capture(&src) {
                Ok((client, capture, event)) => {
                    if let Some(tx) = ready_tx.take() {
                        let _ = tx.send(Ok(()));
                    }
                    capture_loop(&capture, &event, &tap, src.bus(), &running);
                    let _ = client.stop_stream();
                }
                Err(e) => {
                    if let Some(tx) = ready_tx.take() {
                        let _ = tx.send(Err(e.to_string()));
                    }
                    thread::sleep(Duration::from_secs(2));
                }
            }
        }
    });
    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(format!("{}: {} (yritetään taustalla uudelleen)", label, e)),
        Err(_) => Err(format!("{}: ei vastannut ajoissa", label)),
    }
}

fn open_capture(
    src: &Source,
) -> Result<(AudioClient, AudioCaptureClient, Handle), Box<dyn std::error::Error>> {
    let mut client = match src {
        Source::App(name) => AudioClient::new_application_loopback_client(find_process(name)?, true)?,
        Source::Device(dir, name) => find_device(dir, name)?.get_iaudioclient()?,
    };
    let format = WaveFormat::new(32, 32, &SampleType::Float, SAMPLE_RATE as usize, CHANNELS, None);
    // Render device + Capture direction = loopback. Autoconvert lets Windows resample everything to 48 kHz stereo.
    client.initialize_client(
        &format,
        &Direction::Capture,
        &StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: 0 },
    )?;
    let event = client.set_get_eventhandle()?;
    let capture = client.get_audiocaptureclient()?;
    client.start_stream()?;
    Ok((client, capture, event))
}

/// Reads packets until stopped or the device fails (then the caller reopens it).
fn capture_loop(capture: &AudioCaptureClient, event: &Handle, tap: &AudioTap, bus: Bus, running: &AtomicBool) {
    let mut bytes = Vec::new();
    let mut samples = Vec::new();
    let mut next_frame: Option<i64> = None;
    while running.load(Ordering::SeqCst) {
        // Loopback signals nothing while the device is silent, so the timeout doubles as a stop check.
        let _ = event.wait_for_event(50);
        loop {
            let frames = match capture.get_next_packet_size() {
                Ok(Some(n)) if n > 0 => n as usize,
                Ok(_) => break,
                Err(_) => return,
            };
            bytes.resize(frames * CHANNELS * 4, 0);
            let Ok((read, info)) = capture.read_from_device(&mut bytes) else { return };
            let read = read as usize;

            samples.clear();
            if info.flags.silent {
                samples.resize(read * CHANNELS, 0.0);
            } else {
                samples.extend(
                    bytes[..read * CHANNELS * 4]
                        .chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                );
            }

            let ts = if info.timestamp > 0 {
                info.timestamp as i64
            } else {
                now_hns() - read as i64 * HNS_PER_SEC / SAMPLE_RATE
            };
            let mut at = frame_at(ts);
            // Keep packets contiguous unless the clock really jumped (>20 ms): timestamp jitter would click.
            if let Some(expected) = next_frame {
                if (at - expected).abs() < SAMPLE_RATE / 50 {
                    at = expected;
                }
            }
            next_frame = Some(at + read as i64);
            tap.mix(bus, at, &samples);
        }
    }
}

fn find_device(dir: &Direction, name: &str) -> Result<wasapi::Device, wasapi::WasapiError> {
    let enumerator = DeviceEnumerator::new()?;
    if name != DEFAULT_DEVICE {
        if let Ok(dev) = enumerator.get_device_collection(dir)?.get_device_with_name(name) {
            return Ok(dev);
        }
    }
    enumerator.get_default_device(dir)
}

/// PID of the app's root process with this exe name. Works without a window too (e.g. Discord
/// minimized to the tray). Capturing the root's process tree gets audio from all its children.
fn find_process(exe: &str) -> Result<u32, String> {
    if exe.is_empty() {
        return Err("sovellusta ei ole valittu".to_string());
    }
    root_process(&crate::win::processes(), exe).ok_or_else(|| "sovellus ei ole käynnissä".to_string())
}

/// From (pid, parent pid, exe) rows: a process named `exe` whose parent is not also `exe`.
fn root_process(procs: &[(u32, u32, String)], exe: &str) -> Option<u32> {
    let is_named = |pid: u32| procs.iter().any(|(p, _, n)| *p == pid && n.eq_ignore_ascii_case(exe));
    procs
        .iter()
        .find(|(_, parent, name)| name.eq_ignore_ascii_case(exe) && !is_named(*parent))
        .map(|(pid, _, _)| *pid)
}

/// Lists active output (`Render`) or input (`Capture`) device names, default device first.
pub fn list_devices(dir: Direction) -> Vec<String> {
    // Own thread: COM must be MTA here, while the UI thread is already STA.
    thread::spawn(move || {
        let _ = wasapi::initialize_mta();
        let mut names = vec![DEFAULT_DEVICE.to_string()];
        if let Ok(devices) = DeviceEnumerator::new().and_then(|e| e.get_device_collection(&dir)) {
            names.extend((&devices).into_iter().filter_map(|d| d.ok()?.get_friendlyname().ok()));
        }
        names
    })
    .join()
    .unwrap_or_else(|_| vec![DEFAULT_DEVICE.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_mixes_aligns_and_trims() {
        let mut ring = Ring { first_frame: 0, samples: VecDeque::new(), max_frames: 4 };
        ring.mix(10, &[0.5, 0.5, 0.5, 0.5]); // frames 10..12
        ring.mix(11, &[0.25, 0.25, 0.25, 0.25]); // overlaps frame 11, extends to 13
        assert_eq!(ring.first_frame, 10);
        let half = (0.5 * i16::MAX as f32) as i16;
        let quarter = (0.25 * i16::MAX as f32) as i16;
        assert_eq!(ring.samples, [half, half, half + quarter, half + quarter, quarter, quarter]);
        ring.mix(13, &[0.0; 6]); // frames 13..16 -> 6 frames total, trimmed to 4
        assert_eq!(ring.first_frame, 12);
        assert_eq!(ring.samples.len(), 8);
        ring.mix(1000, &[1.0, 1.0]); // far future: old data dropped
        assert_eq!((ring.first_frame, ring.samples.len()), (1000, 2));
    }

    #[test]
    fn root_process_skips_same_named_children() {
        let procs = [
            (10, 1, "explorer.exe".to_string()),
            (30, 20, "Discord.exe".to_string()), // child listed before its parent
            (20, 10, "Discord.exe".to_string()),
        ];
        assert_eq!(root_process(&procs, "discord.exe"), Some(20));
        assert_eq!(root_process(&procs, "game.exe"), None);
    }

    #[test]
    fn wav_pads_silence_and_mixes_buses() {
        let tap = AudioTap::new(1);
        let hns_of = |frame: i64| (frame * HNS_PER_SEC + SAMPLE_RATE - 1) / SAMPLE_RATE;
        tap.mix(Bus::Desktop, 48_000, &[0.5, 0.5]);
        tap.mix(Bus::Mic, 48_000, &[0.25, 0.25, 0.25, 0.25]);
        let path = std::env::temp_dir().join("silly_borg_test.wav");
        let pcm_of = |track| {
            // From one frame before the data to one frame after the desktop audio: 3 frames.
            tap.write_wav(hns_of(47_999), hns_of(48_002), track, &path).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            bytes[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect::<Vec<i16>>()
        };
        let (half, quarter) = ((0.5 * i16::MAX as f32) as i16, (0.25 * i16::MAX as f32) as i16);
        assert_eq!(pcm_of(Track::Only(Bus::Desktop)), [0, 0, half, half, 0, 0]);
        assert_eq!(pcm_of(Track::Only(Bus::Mic)), [0, 0, quarter, quarter, quarter, quarter]);
        assert_eq!(pcm_of(Track::Mix), [0, 0, half + quarter, half + quarter, quarter, quarter]);
        let _ = std::fs::remove_file(path);
    }

    /// Manual check against real hardware: `cargo test -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn captures_default_output_and_mic() {
        let mut cfg = AppConfig::default();
        cfg.record_microphone = true;
        let tap = AudioTap::new(10);
        let rec = AudioRecorder::start(&cfg, &tap);
        assert!(rec.warnings.is_empty(), "{:?}", rec.warnings);
        thread::sleep(Duration::from_secs(2));
        let ring = tap.0[Bus::Desktop as usize].lock();
        let end_frame = ring.first_frame + (ring.samples.len() / CHANNELS) as i64;
        let lag_ms = (frame_at(now_hns()) - end_frame) * 1000 / SAMPLE_RATE;
        let peak = ring.samples.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        let report = format!("len={} lag_ms={} peak={}", ring.samples.len(), lag_ms, peak);
        let _ = std::fs::write(std::env::temp_dir().join("silly_borg_audio.txt"), &report);
        assert!(ring.samples.len() > 0 && (-100..500).contains(&lag_ms), "{}", report);
    }

    /// Manual: `SILLYBORG_TEST_APP=Discord.exe cargo test app_loopback -- --ignored`
    #[test]
    #[ignore]
    fn app_loopback_opens() {
        let mut cfg = AppConfig::default();
        cfg.desktop_audio = DesktopAudioMode::Application;
        cfg.app_audio_process = std::env::var("SILLYBORG_TEST_APP").unwrap();
        let rec = AudioRecorder::start(&cfg, &AudioTap::new(10));
        assert!(rec.warnings.is_empty(), "{:?}", rec.warnings);
    }
}
