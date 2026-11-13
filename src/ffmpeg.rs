//! ffmpeg and ffprobe on demand. If they are not next to the exe or on PATH, the app downloads a
//! pinned build once, checks its SHA-256 and unpacks only the two exes into
//! `%LOCALAPPDATA%\TiksuClipper\ffmpeg`. ffmpeg is GPL software and stays a separate program:
//! this app downloads and runs it, it does not ship or link it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use parking_lot::Mutex;
use sha2::{Digest, Sha256};

const URL: &str = "https://github.com/GyanD/codexffmpeg/releases/download/9.0.2/ffmpeg-9.0.2-essentials_build.7z";
/// SHA-256 of the archive above (matches the digest GitHub shows for the release asset).
const SHA256: &str = "4705843ccaaf54257c16ad90f3e952ece33c17df964ecf7bfdbb0f49c7171077";
const SIZE: u64 = 35_430_500;

#[derive(Debug, Clone, PartialEq)]
pub enum Setup {
    Idle,
    /// Percent downloaded.
    Downloading(u8),
    Unpacking,
    Failed(String),
}

static STATE: Mutex<Setup> = Mutex::new(Setup::Idle);

pub fn state() -> Setup {
    STATE.lock().clone()
}

/// Where the downloaded tools live.
pub fn tool_dir() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join("TiksuClipper").join("ffmpeg"))
}

/// Downloads and installs ffmpeg in the background; `done(true)` when it is ready to use.
/// Does nothing if a download is already running.
pub fn install(done: impl FnOnce(bool) + Send + 'static) {
    {
        let mut state = STATE.lock();
        if matches!(*state, Setup::Downloading(_) | Setup::Unpacking) {
            return;
        }
        *state = Setup::Downloading(0);
    }
    std::thread::spawn(move || {
        let result = run();
        match &result {
            Ok(()) => {
                println!("ffmpeg asennettu");
                *STATE.lock() = Setup::Idle;
            }
            Err(e) => {
                println!("ffmpegin asennus epäonnistui: {}", e);
                *STATE.lock() = Setup::Failed(e.clone());
            }
        }
        done(result.is_ok());
    });
}

fn run() -> Result<(), String> {
    let dir = tool_dir().ok_or("käyttäjän tietokansiota ei löytynyt")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    let archive = dir.join("download.7z.part");
    let unpacked = dir.join("unpack");
    let _ = std::fs::remove_dir_all(&unpacked);

    let result = download(&archive).and_then(|_| unpack(&archive, &unpacked, &dir));
    let _ = std::fs::remove_file(&archive);
    let _ = std::fs::remove_dir_all(&unpacked);
    result?;

    // Last check: the unpacked program really starts.
    let ok = Command::new(dir.join("ffmpeg.exe")).arg("-version").output().is_ok_and(|o| o.status.success());
    if ok { Ok(()) } else { Err("ladattu ffmpeg ei käynnisty".to_string()) }
}

/// curl writes `to`; the file size gives the progress.
fn download(to: &Path) -> Result<(), String> {
    let _ = std::fs::remove_file(to);
    let mut child = crate::update::curl("1800")
        .arg("-o")
        .arg(to)
        .arg(URL)
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl.exe ei käynnistynyt: {}", e))?;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        let got = std::fs::metadata(to).map_or(0, |m| m.len());
        *STATE.lock() = Setup::Downloading((got * 100 / SIZE).min(99) as u8);
        std::thread::sleep(Duration::from_millis(400));
    };
    if !status.success() {
        let mut err = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut err);
        }
        return Err(format!("lataus epäonnistui: {}", err.trim().lines().last().unwrap_or("tuntematon virhe")));
    }

    let bytes = std::fs::read(to).map_err(|e| e.to_string())?;
    let hash: String = Sha256::digest(&bytes).iter().map(|b| format!("{:02x}", b)).collect();
    if hash == SHA256 {
        Ok(())
    } else {
        Err("ladatun tiedoston tarkiste ei täsmää, tiedostoa ei käytetä".to_string())
    }
}

/// Windows' own tar.exe reads 7z; only the two exes are pulled out of the archive.
fn unpack(archive: &Path, unpacked: &Path, dir: &Path) -> Result<(), String> {
    *STATE.lock() = Setup::Unpacking;
    std::fs::create_dir_all(unpacked).map_err(|e| e.to_string())?;
    let system_tar = std::env::var_os("SystemRoot").map(|r| PathBuf::from(r).join("System32").join("tar.exe"));
    let mut cmd = Command::new(system_tar.filter(|p| p.exists()).unwrap_or_else(|| PathBuf::from("tar.exe")));
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000);
    let out = cmd
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(unpacked)
        .args(["*/bin/ffprobe.exe", "*/bin/ffmpeg.exe"])
        .output()
        .map_err(|e| format!("tar.exe ei käynnistynyt: {}", e))?;
    if !out.status.success() {
        return Err(format!("purku epäonnistui: {}", String::from_utf8_lossy(&out.stderr).trim().lines().last().unwrap_or("")));
    }
    // ffmpeg.exe goes last: its presence means both tools are in place.
    for name in ["ffprobe.exe", "ffmpeg.exe"] {
        let from = find(unpacked, name).ok_or_else(|| format!("{} puuttui paketista", name))?;
        let _ = std::fs::remove_file(dir.join(name));
        std::fs::rename(&from, dir.join(name)).map_err(|e| format!("{}: {}", name, e))?;
    }
    Ok(())
}

/// `name` one or two folders below `root` (the archive wraps everything in a versioned folder).
fn find(root: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n.eq_ignore_ascii_case(name)) {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_looks_inside_nested_folders() {
        let root = std::env::temp_dir().join("silly_borg_ffmpeg_find");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("ffmpeg-9.0.2-essentials_build").join("bin")).unwrap();
        std::fs::write(root.join("ffmpeg-9.0.2-essentials_build").join("bin").join("ffmpeg.exe"), b"x").unwrap();
        assert!(find(&root, "ffmpeg.exe").is_some());
        assert!(find(&root, "ffprobe.exe").is_none());
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(test)]
mod network_tests {
    use super::*;

    /// Manual, needs internet: `cargo test real_download -- --ignored`. Downloads, checks the hash and
    /// unpacks, but does not run the downloaded programs.
    #[test]
    #[ignore]
    fn real_download_unpacks_both_tools() {
        let dir = std::env::temp_dir().join("silly_borg_ffmpeg_download");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("download.7z.part");
        download(&archive).unwrap();
        unpack(&archive, &dir.join("unpack"), &dir).unwrap();
        for name in ["ffmpeg.exe", "ffprobe.exe"] {
            assert!(std::fs::metadata(dir.join(name)).unwrap().len() > 10_000_000, "{}", name);
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
