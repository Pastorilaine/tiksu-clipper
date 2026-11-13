//! Automatic updates. `latest.json` is read from GitHub Releases and tiksu.fi (the newer one
//! wins), the exe is downloaded, its ed25519 signature checked, and the running exe is swapped
//! out (Windows allows renaming a running exe). The new version starts on the next launch,
//! or right away via [`restart`].

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use parking_lot::Mutex;

use crate::manifest::{parse_version, verify, Manifest};

/// Update manifests, newest version wins. Both serve the same signed `latest.json`.
const SOURCES: [&str; 2] = [
    "https://github.com/Pastorilaine/tiksu-clipper/releases/latest/download/latest.json",
    "https://tiksu.fi/clipper/latest.json",
];
/// Matches the release signing key (`tiksu-release keygen`). Only builds signed with it install.
const PUBLIC_KEY: &str = "9f11973a4acc0d0fc7456d75363c03a0ab1291c98a12d610e9a0cb378fdef54d";
pub const DOWNLOAD_PAGE: &str = "https://github.com/Pastorilaine/tiksu-clipper/releases/latest";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

const FIRST_CHECK: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(15 * 60);
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    /// Found, waiting for the user (automatic install is off).
    Available(Manifest),
    Downloading(Manifest),
    /// Swapped in; runs after a restart.
    Installed(Manifest),
    Failed(String),
}

pub struct Updater {
    state: Mutex<UpdateState>,
    busy: AtomicBool,
    /// Install found updates without asking.
    auto: AtomicBool,
    repaint: OnceLock<eframe::egui::Context>,
}

impl Updater {
    pub fn new(auto: bool) -> Arc<Self> {
        let updater = Arc::new(Self {
            state: Mutex::new(UpdateState::Idle),
            busy: AtomicBool::new(false),
            auto: AtomicBool::new(auto),
            repaint: OnceLock::new(),
        });
        let bg = updater.clone();
        std::thread::spawn(move || {
            std::thread::sleep(FIRST_CHECK);
            loop {
                // With automatic install off it still checks, so the app can tell about the new version.
                bg.run(bg.auto.load(Ordering::SeqCst));
                // A failed check (offline after wake-up, GitHub hiccup) is retried soon, not in 6 hours.
                let wait = if matches!(bg.state(), UpdateState::Failed(_)) { RETRY_AFTER_FAILURE } else { CHECK_EVERY };
                std::thread::sleep(wait);
            }
        });
        updater
    }

    pub fn set_repaint_context(&self, ctx: eframe::egui::Context) {
        let _ = self.repaint.set(ctx);
    }

    pub fn set_auto(&self, auto: bool) {
        self.auto.store(auto, Ordering::SeqCst);
    }

    pub fn state(&self) -> UpdateState {
        self.state.lock().clone()
    }

    fn set_state(&self, state: UpdateState) {
        *self.state.lock() = state;
        if let Some(ctx) = self.repaint.get() {
            ctx.request_repaint();
        }
    }

    /// Checks in the background; `install` also downloads and installs what it finds.
    pub fn check(self: &Arc<Self>, install: bool) {
        let me = self.clone();
        std::thread::spawn(move || me.run(install));
    }

    fn run(&self, install: bool) {
        if self.busy.swap(true, Ordering::SeqCst) {
            return;
        }
        // An installed update stays installed until the restart; don't download it again.
        if !matches!(self.state(), UpdateState::Installed(_)) {
            self.set_state(UpdateState::Checking);
            // Test or staging source; the signature check still applies.
            let custom = std::env::var("TIKSU_UPDATE_SOURCE").ok();
            let sources: Vec<&str> = match &custom {
                Some(url) => vec![url.as_str()],
                None => SOURCES.to_vec(),
            };
            let state = match latest(&sources) {
                Err(e) => UpdateState::Failed(e),
                Ok(None) => UpdateState::UpToDate,
                Ok(Some(m)) if !install => UpdateState::Available(m),
                Ok(Some(m)) => {
                    println!("Päivitys {} löytyi, ladataan", m.version);
                    self.set_state(UpdateState::Downloading(m.clone()));
                    let exe = std::env::current_exe().map_err(|e| e.to_string());
                    match exe.and_then(|exe| download_and_install(&m, PUBLIC_KEY, &exe)) {
                        Ok(()) => UpdateState::Installed(m),
                        Err(e) => UpdateState::Failed(e),
                    }
                }
            };
            match &state {
                UpdateState::Failed(e) => println!("Päivitys: {}", e),
                UpdateState::Installed(m) => println!("Päivitys {} asennettu, otetaan käyttöön uudelleenkäynnistyksessä", m.version),
                _ => {}
            }
            self.set_state(state);
        }
        self.busy.store(false, Ordering::SeqCst);
    }
}

/// The system's curl (TLS and redirects via the OS), not whatever curl.exe sits next to our exe
/// (often the Downloads folder). `max_time` is in seconds.
pub fn curl(max_time: &str) -> Command {
    let system_curl = std::env::var_os("SystemRoot").map(|r| PathBuf::from(r).join("System32").join("curl.exe"));
    let mut cmd = Command::new(system_curl.filter(|p| p.exists()).unwrap_or_else(|| PathBuf::from("curl.exe")));
    cmd.creation_flags(CREATE_NO_WINDOW)
        .args(["-fsSL", "--retry", "2", "--max-time", max_time])
        .args(["-A", &format!("TiksuClipper/{}", CURRENT)]);
    cmd
}

/// Downloads `url` with [`curl`]; into `to` if given, else returns the body.
fn fetch(url: &str, to: Option<&Path>) -> Result<Vec<u8>, String> {
    let mut cmd = curl(if to.is_some() { "600" } else { "20" });
    if let Some(to) = to {
        cmd.arg("-o").arg(to);
    }
    let out = cmd.arg(url).output().map_err(|e| format!("curl.exe ei käynnistynyt: {}", e))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("{}: {}", url, err.trim().lines().last().unwrap_or("lataus epäonnistui")))
    }
}

fn is_newer(version: &str) -> bool {
    matches!((parse_version(version), parse_version(CURRENT)), (Some(new), Some(cur)) if new > cur)
}

/// The newest manifest above the running version. Errors only if every source failed.
fn latest(sources: &[&str]) -> Result<Option<Manifest>, String> {
    let mut best: Option<Manifest> = None;
    let mut errors = Vec::new();
    for src in sources {
        let manifest = fetch(src, None).and_then(|json| {
            serde_json::from_slice::<Manifest>(&json).map_err(|e| format!("{}: virheellinen latest.json ({})", src, e))
        });
        match manifest {
            Ok(m) if is_newer(&m.version) && best.as_ref().is_none_or(|b| parse_version(&m.version) > parse_version(&b.version)) => {
                best = Some(m)
            }
            Ok(_) => {}
            Err(e) => errors.push(e),
        }
    }
    if best.is_none() && errors.len() == sources.len() {
        return Err(format!("Päivityksiä ei voitu tarkistaa: {}", errors.join("; ")));
    }
    Ok(best)
}

fn download_and_install(m: &Manifest, public_key: &str, exe: &Path) -> Result<(), String> {
    let tmp = std::env::temp_dir().join(format!("TiksuClipper-{}.download", m.version));
    let result = fetch(&m.url, Some(&tmp)).and_then(|_| {
        let bytes = std::fs::read(&tmp).map_err(|e| e.to_string())?;
        verify(m, &bytes, public_key)?;
        install(&bytes, exe)
    });
    let _ = std::fs::remove_file(&tmp);
    result
}

/// Where the previous exe goes while the new one takes its place.
pub fn old_path(exe: &Path) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".old");
    exe.with_file_name(name)
}

/// Puts `bytes` at `exe`, moving the (possibly running) current exe aside.
fn install(bytes: &[u8], exe: &Path) -> Result<(), String> {
    if !exe.exists() {
        return Err(format!(
            "Ohjelman tiedosto on siirretty tai poistettu käynnissä ollessa ({}). Sulje sovellus ja käynnistä se uudelleen sen oikeasta sijainnista, tai lataa uusi versio: {}",
            exe.display(),
            DOWNLOAD_PAGE
        ));
    }
    let old = old_path(exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| {
        format!(
            "Ohjelman kansioon ei voi kirjoittaa ({}). Lataa uusi versio käsin: {}",
            e, DOWNLOAD_PAGE
        )
    })?;
    if let Err(e) = std::fs::write(exe, bytes) {
        let _ = std::fs::rename(&old, exe);
        return Err(format!("Uuden version kirjoitus epäonnistui: {}", e));
    }
    Ok(())
}

/// Removes the exe left behind by the previous update (it can't be deleted while it runs).
pub fn cleanup_old() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_path(&exe));
    }
}

/// Starts the freshly installed exe; it waits for this process to exit before starting up.
pub fn restart(hidden: bool) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.args(["--wait-pid", &std::process::id().to_string(), "--updated"]);
    if hidden {
        cmd.arg("--tray");
    }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{signed_message, to_hex};
    use ed25519_dalek::{Signer, SigningKey};

    fn file_url(path: &Path) -> String {
        format!("file:///{}", path.to_string_lossy().replace('\\', "/"))
    }

    /// Full flow against local files: manifest via curl, download, signature check, exe swap.
    #[test]
    fn downloads_verifies_and_swaps_exe() {
        let dir = std::env::temp_dir().join("silly_borg_update_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let key = SigningKey::from_bytes(&[3u8; 32]);
        let public = to_hex(key.verifying_key().as_bytes());
        let new_exe = b"MZ new version".to_vec();
        std::fs::write(dir.join("new.exe"), &new_exe).unwrap();
        let manifest = Manifest {
            version: "99.0.0".into(),
            url: file_url(&dir.join("new.exe")),
            notes: "Uutta".into(),
            signature: to_hex(&key.sign(&signed_message("99.0.0", &new_exe)).to_bytes()),
        };
        std::fs::write(dir.join("latest.json"), serde_json::to_string(&manifest).unwrap()).unwrap();
        let older = Manifest { version: "0.0.1".into(), ..manifest.clone() };
        std::fs::write(dir.join("older.json"), serde_json::to_string(&older).unwrap()).unwrap();

        // Newest of the sources wins; a dead source doesn't matter while another answers.
        let sources = [file_url(&dir.join("older.json")), file_url(&dir.join("missing.json")), file_url(&dir.join("latest.json"))];
        let sources: Vec<&str> = sources.iter().map(String::as_str).collect();
        assert_eq!(latest(&sources).unwrap(), Some(manifest.clone()));
        assert_eq!(latest(&sources[..1]).unwrap(), None);
        assert!(latest(&sources[1..2]).is_err());

        let exe = dir.join("TiksuClipper.exe");
        std::fs::write(&exe, b"MZ old version").unwrap();
        download_and_install(&manifest, &public, &exe).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), new_exe);
        assert_eq!(std::fs::read(old_path(&exe)).unwrap(), b"MZ old version");

        // Signed by someone else: refused, current exe untouched.
        let other = to_hex(SigningKey::from_bytes(&[4u8; 32]).verifying_key().as_bytes());
        assert!(download_and_install(&manifest, &other, &exe).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), new_exe);
        let _ = std::fs::remove_dir_all(dir);
    }
}
