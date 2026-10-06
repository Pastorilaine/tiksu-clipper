use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::audio::DEFAULT_DEVICE;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CaptureTargetType {
    #[default]
    Monitor,
    Window,
}

/// Mitkä tietokoneen äänet tallennetaan mikrofonin lisäksi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DesktopAudioMode {
    /// Ei tietokoneen ääniä
    Off,
    /// Kaikki valitun äänilaitteen (kaiuttimet/kuulokkeet) äänet
    #[default]
    Device,
    /// Vain yhden sovelluksen äänet
    Application,
}

/// Lisäpikanäppäin, joka tallentaa eri pituisen leikkeen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtraHotkey {
    pub hotkey: String,
    pub secs: u64,
}

/// Pelikohtaiset asetukset, jotka otetaan käyttöön kun peli (exe) on käynnissä.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameProfile {
    pub exe: String,
    /// Kuvataanko vain pelin ikkuna (muuten perusasetusten kuvauslähde)
    pub capture_window: bool,
    pub frame_rate: u32,
    pub bitrate: u32,
    pub clip_duration_secs: u64,
}

fn default_monitor_index() -> usize {
    1
}
fn default_clip_duration_secs() -> u64 {
    120
}
fn default_segment_duration_secs() -> u64 {
    10
}
fn default_hotkey() -> String {
    "F10".to_string()
}
fn default_bitrate() -> u32 {
    20_000_000
}
fn default_frame_rate() -> u32 {
    60
}
fn default_true() -> bool {
    true
}
fn default_audio_device() -> String {
    DEFAULT_DEVICE.to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    /// Kuvauslähde: Koko näyttö vai sovellusikkuna
    #[serde(default)]
    pub capture_source_type: CaptureTargetType,

    /// Valitun näytön numero (1-pohjainen)
    #[serde(default = "default_monitor_index")]
    pub monitor_index: usize,

    /// Valitun ikkunan otsikko (varalla, jos prosessia ei löydy)
    #[serde(default)]
    pub window_title: String,

    /// Valitun ikkunan prosessi (esim. "cs2.exe"); ikkuna haetaan ensisijaisesti tällä
    #[serde(default)]
    pub window_process: String,

    /// Klipin pituus sekunneissa (oletus: 120 = 2 min)
    #[serde(default = "default_clip_duration_secs")]
    pub clip_duration_secs: u64,

    /// Yksittäisen siivun pituus (oletus: 10s)
    #[serde(default = "default_segment_duration_secs")]
    pub segment_duration_secs: u64,

    /// Pidetäänkö puskuri muistissa (true) vai väliaikaistiedostoissa levyllä
    #[serde(default = "default_true")]
    pub buffer_in_memory: bool,

    /// Tallennuskansio valmiille klipeille
    pub output_dir: PathBuf,

    /// Pikanäppäin klipin tallentamiseen (oletus: F10)
    #[serde(default = "default_hotkey")]
    pub hotkey: String,

    /// Lisäpikanäppäimet eri pituisille klipeille
    #[serde(default)]
    pub extra_hotkeys: Vec<ExtraHotkey>,

    /// Bittinopeus bitteinä / sekunti (oletus: 20 Mbps)
    #[serde(default = "default_bitrate")]
    pub bitrate: u32,

    /// Kuvanopeus (oletus: 60 FPS)
    #[serde(default = "default_frame_rate")]
    pub frame_rate: u32,

    /// Soitetaanko merkkiääni klipin tallentuessa
    #[serde(default = "default_true")]
    pub play_sound: bool,

    /// Näytetäänkö ilmoitus ruudun kulmassa klipin tallentuessa
    #[serde(default = "default_true")]
    pub overlay: bool,

    /// Tietokoneen äänet: ei mitään / äänilaite / yksittäinen sovellus
    #[serde(default)]
    pub desktop_audio: DesktopAudioMode,

    /// Äänilaite jonka äänet tallennetaan (kaiuttimet/kuulokkeet)
    #[serde(default = "default_audio_device")]
    pub output_device: String,

    /// Sovelluksen prosessinimi (esim. "Discord.exe"), jonka äänet tallennetaan
    #[serde(default)]
    pub app_audio_process: String,

    /// Tallennetaanko mikrofoni
    #[serde(default)]
    pub record_microphone: bool,

    /// Valitun mikrofonin nimi
    #[serde(default = "default_audio_device")]
    pub microphone_device: String,

    /// Tietokoneen äänet ja mikrofoni myös omille raidoilleen
    #[serde(default)]
    pub separate_audio_tracks: bool,

    /// Käynnistetäänkö Windowsin mukana (ilmaisinalueelle)
    #[serde(default)]
    pub autostart: bool,

    /// Sulkeminen piilottaa ikkunan ilmaisinalueelle, puskuri jatkaa
    #[serde(default = "default_true")]
    pub close_to_tray: bool,

    /// Asennetaanko päivitykset automaattisesti taustalla
    #[serde(default = "default_true")]
    pub auto_update: bool,

    /// Vaihdetaanko peliprofiiliin automaattisesti
    #[serde(default = "default_true")]
    pub auto_profiles: bool,

    #[serde(default)]
    pub profiles: Vec<GameProfile>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let output_dir = dirs::video_dir()
            .map(|p| p.join("Clips"))
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("Videos")
                    .join("Clips")
            });

        Self {
            capture_source_type: CaptureTargetType::Monitor,
            monitor_index: 1,
            window_title: String::new(),
            window_process: String::new(),
            clip_duration_secs: 120,
            segment_duration_secs: 10,
            buffer_in_memory: true,
            output_dir,
            hotkey: default_hotkey(),
            extra_hotkeys: Vec::new(),
            bitrate: 20_000_000,
            frame_rate: 60,
            play_sound: true,
            overlay: true,
            desktop_audio: DesktopAudioMode::Device,
            output_device: default_audio_device(),
            app_audio_process: String::new(),
            record_microphone: false,
            microphone_device: default_audio_device(),
            separate_audio_tracks: false,
            autostart: false,
            close_to_tray: true,
            auto_update: true,
            auto_profiles: true,
            profiles: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .map(|p| p.join("SillyBorg"))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(contents) = fs::read_to_string(&path) {
                match serde_json::from_str::<AppConfig>(&contents) {
                    Ok(cfg) => return cfg,
                    Err(e) => println!("config.json ei kelpaa, käytetään oletuksia: {}", e),
                }
            }
        }

        let default_cfg = Self::default();
        let _ = default_cfg.save();
        default_cfg
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)
    }

    /// Longest clip any hotkey can ask for: the buffer must hold this much.
    pub fn max_clip_secs(&self) -> u64 {
        self.extra_hotkeys.iter().map(|h| h.secs).fold(self.clip_duration_secs, u64::max)
    }

    /// Every hotkey with the clip length it saves; the main one first.
    pub fn all_hotkeys(&self) -> Vec<(String, u64)> {
        std::iter::once((self.hotkey.clone(), self.clip_duration_secs))
            .chain(self.extra_hotkeys.iter().map(|h| (h.hotkey.clone(), h.secs)))
            .collect()
    }

    /// This config with a game profile's overrides applied.
    pub fn with_profile(&self, p: &GameProfile) -> AppConfig {
        let mut cfg = self.clone();
        if p.capture_window {
            cfg.capture_source_type = CaptureTargetType::Window;
            cfg.window_process = p.exe.clone();
            cfg.window_title.clear();
        }
        cfg.frame_rate = p.frame_rate;
        cfg.bitrate = p.bitrate;
        cfg.clip_duration_secs = p.clip_duration_secs;
        cfg
    }

    pub fn profile_for(&self, exe: &str) -> Option<&GameProfile> {
        self.profiles.iter().find(|p| p.exe.eq_ignore_ascii_case(exe))
    }

    /// Settings that need a new capture session (and so drop the buffered video) when changed.
    pub fn video_differs(&self, other: &AppConfig) -> bool {
        (self.capture_source_type, self.monitor_index, &self.window_title, &self.window_process)
            != (other.capture_source_type, other.monitor_index, &other.window_title, &other.window_process)
            || (self.frame_rate, self.bitrate, self.segment_duration_secs, self.buffer_in_memory)
                != (other.frame_rate, other.bitrate, other.segment_duration_secs, other.buffer_in_memory)
    }

    /// Settings that need the audio sources reopened (the audio buffer is kept).
    pub fn audio_differs(&self, other: &AppConfig) -> bool {
        (self.desktop_audio, &self.output_device, &self.app_audio_process, self.record_microphone, &self.microphone_device)
            != (other.desktop_audio, &other.output_device, &other.app_audio_process, other.record_microphone, &other.microphone_device)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_config_still_loads_and_profile_overrides() {
        // A config saved by an older version: unknown field, new fields missing.
        let old = r#"{"output_dir":"C:\\Clips","hotkey":"F9","encoder_backend":"Auto"}"#;
        let mut cfg: AppConfig = serde_json::from_str(old).unwrap();
        assert!(cfg.buffer_in_memory && cfg.close_to_tray && cfg.profiles.is_empty());

        cfg.extra_hotkeys.push(ExtraHotkey { hotkey: "F8".into(), secs: 300 });
        assert_eq!(cfg.max_clip_secs(), 300);
        assert_eq!(cfg.all_hotkeys()[0], ("F9".to_string(), 120));

        let p = GameProfile { exe: "cs2.exe".into(), capture_window: true, frame_rate: 120, bitrate: 60_000_000, clip_duration_secs: 30 };
        let eff = cfg.with_profile(&p);
        assert_eq!((eff.capture_source_type, eff.window_process.as_str(), eff.frame_rate), (CaptureTargetType::Window, "cs2.exe", 120));
        assert!(eff.video_differs(&cfg) && !eff.audio_differs(&cfg));
    }
}
