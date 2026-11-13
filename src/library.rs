//! Saved clips: listing, thumbnails and durations (ffmpeg on a worker thread), trimming.

use std::collections::{HashMap, HashSet, VecDeque};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::SystemTime;

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

use crate::muxer;

/// Newest clips shown; older ones are still in the folder.
const MAX_CLIPS: usize = 60;
/// ponytail: texture cache is dropped wholesale past this; an LRU if scrubbing ever feels slow
const MAX_TEXTURES: usize = 400;

#[derive(Debug, Clone)]
pub struct ClipEntry {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub modified: SystemTime,
}

/// A frame of a clip at a given time and size.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThumbKey {
    pub path: PathBuf,
    pub millis: u64,
    pub w: u32,
    pub h: u32,
}

enum Job {
    Thumb(ThumbKey),
    Duration(PathBuf),
}

enum Done {
    Thumb(ThumbKey, Option<Vec<u8>>),
    Duration(PathBuf, Option<f64>),
    Trim(Result<PathBuf, String>),
}

/// Newest MP4s in `dir`, newest first.
pub fn scan(dir: &Path) -> Vec<ClipEntry> {
    let mut clips: Vec<ClipEntry> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("mp4")))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            Some(ClipEntry {
                name: e.file_name().to_string_lossy().to_string(),
                path: e.path(),
                size: meta.len(),
                modified: meta.modified().ok()?,
            })
        })
        .collect();
    clips.sort_by_key(|c| std::cmp::Reverse(c.modified));
    clips.truncate(MAX_CLIPS);
    clips
}

pub fn open(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}

pub fn show_in_folder(path: &Path) {
    let _ = std::process::Command::new("explorer")
        .raw_arg(format!("/select,\"{}\"", path.display()))
        .spawn();
}

/// `name_trim.mp4`, `name_trim2.mp4`, … whichever is free.
pub fn trim_output(input: &Path) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    (1..)
        .map(|n| {
            let suffix = if n == 1 { String::new() } else { n.to_string() };
            input.with_file_name(format!("{}_trim{}.mp4", stem, suffix))
        })
        .find(|p| !p.exists())
        .unwrap()
}

pub struct Library {
    pub clips: Vec<ClipEntry>,
    scanned: Option<(PathBuf, u64)>,
    jobs: Arc<(Mutex<VecDeque<Job>>, Condvar)>,
    done_tx: Sender<Done>,
    done_rx: Receiver<Done>,
    ctx: egui::Context,
    thumbs: HashMap<ThumbKey, Option<TextureHandle>>,
    durations: HashMap<PathBuf, Option<f64>>,
    requested: HashSet<ThumbKey>,
    pub trimming: bool,
    /// Result of the last trim, taken by the UI to show a toast.
    pub trim_result: Option<Result<PathBuf, String>>,
}

impl Library {
    pub fn new(ctx: egui::Context) -> Self {
        let jobs: Arc<(Mutex<VecDeque<Job>>, Condvar)> = Arc::default();
        let (done_tx, done_rx) = channel();
        {
            let (jobs, done_tx, ctx) = (jobs.clone(), done_tx.clone(), ctx.clone());
            std::thread::spawn(move || loop {
                let job = {
                    let (queue, cv) = &*jobs;
                    let mut q = cv.wait_while(queue.lock().unwrap(), |q| q.is_empty()).unwrap();
                    // Newest request first: the frame the user just asked for beats a queued strip.
                    q.pop_back().unwrap()
                };
                let done = match job {
                    Job::Thumb(key) => {
                        let rgba = muxer::thumbnail(&key.path, key.millis as f64 / 1000.0, key.w, key.h).ok();
                        Done::Thumb(key, rgba)
                    }
                    Job::Duration(path) => {
                        let d = muxer::duration(&path);
                        Done::Duration(path, d)
                    }
                };
                if done_tx.send(done).is_err() {
                    break;
                }
                ctx.request_repaint();
            });
        }
        Self {
            clips: Vec::new(),
            scanned: None,
            jobs,
            done_tx,
            done_rx,
            ctx,
            thumbs: HashMap::new(),
            durations: HashMap::new(),
            requested: HashSet::new(),
            trimming: false,
            trim_result: None,
        }
    }

    fn push(&self, job: Job) {
        let (queue, cv) = &*self.jobs;
        queue.lock().unwrap().push_back(job);
        cv.notify_one();
    }

    /// Rescans when the folder changed or a new clip was saved (`saves` counter).
    pub fn refresh(&mut self, dir: &Path, saves: u64, force: bool) {
        let key = (dir.to_path_buf(), saves);
        if force || self.scanned.as_ref() != Some(&key) {
            self.clips = scan(dir);
            self.scanned = Some(key);
        }
    }

    /// Applies finished worker results. Call once per frame.
    pub fn update(&mut self) {
        while let Ok(done) = self.done_rx.try_recv() {
            match done {
                Done::Thumb(key, rgba) => {
                    if self.thumbs.len() > MAX_TEXTURES {
                        self.thumbs.clear();
                        self.requested.clear();
                    }
                    let tex = rgba.map(|rgba| {
                        let img = ColorImage::from_rgba_unmultiplied([key.w as usize, key.h as usize], &rgba);
                        self.ctx.load_texture(format!("{:?}", key), img, TextureOptions::LINEAR)
                    });
                    self.thumbs.insert(key, tex);
                }
                Done::Duration(path, d) => {
                    self.durations.insert(path, d);
                }
                Done::Trim(result) => {
                    self.trimming = false;
                    if let Ok(path) = &result {
                        self.forget(path);
                    }
                    self.scanned = None;
                    self.trim_result = Some(result);
                }
            }
        }
    }

    /// Frame of `path` at `at` seconds; requested in the background on first use.
    pub fn thumb(&mut self, path: &Path, at: f64, w: u32, h: u32) -> Option<TextureHandle> {
        // 100 ms buckets: scrubbing reuses nearby frames instead of queueing one per pixel.
        let key = ThumbKey { path: path.to_path_buf(), millis: (at.max(0.0) * 10.0).round() as u64 * 100, w, h };
        if let Some(tex) = self.thumbs.get(&key) {
            return tex.clone();
        }
        if self.requested.insert(key.clone()) {
            self.push(Job::Thumb(key));
        }
        None
    }

    /// Any already-loaded frame of this clip at this size (shown while the exact one loads).
    pub fn nearest_thumb(&self, path: &Path, at: f64, w: u32, h: u32) -> Option<TextureHandle> {
        let millis = (at.max(0.0) * 1000.0) as i64;
        self.thumbs
            .iter()
            .filter(|(k, t)| k.path == path && k.w == w && k.h == h && t.is_some())
            .min_by_key(|(k, _)| (k.millis as i64 - millis).abs())
            .and_then(|(_, t)| t.clone())
    }

    pub fn duration(&mut self, path: &Path) -> Option<f64> {
        if let Some(d) = self.durations.get(path) {
            return *d;
        }
        self.durations.insert(path.to_path_buf(), None);
        self.push(Job::Duration(path.to_path_buf()));
        None
    }

    fn forget(&mut self, path: &Path) {
        self.thumbs.retain(|k, _| k.path != path);
        self.requested.retain(|k| k.path != path);
        self.durations.remove(path);
    }

    pub fn delete(&mut self, path: &Path) -> Result<(), String> {
        crate::win::recycle(path)?;
        self.forget(path);
        self.clips.retain(|c| c.path != path);
        Ok(())
    }

    /// Cuts `[start, end]` s into a new file next to the original, in the background.
    pub fn trim(&mut self, input: &Path, start: f64, end: f64) {
        self.trimming = true;
        let (input, tx, ctx) = (input.to_path_buf(), self.done_tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let output = trim_output(&input);
            let result = muxer::trim(&input, start, end, &output).map(|_| output);
            let _ = tx.send(Done::Trim(result));
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_output_picks_free_name() {
        let dir = std::env::temp_dir().join("silly_borg_trim_name");
        let _ = std::fs::create_dir_all(&dir);
        let input = dir.join("clip_a.mp4");
        let _ = std::fs::remove_file(dir.join("clip_a_trim.mp4"));
        assert_eq!(trim_output(&input), dir.join("clip_a_trim.mp4"));
        std::fs::write(dir.join("clip_a_trim.mp4"), b"x").unwrap();
        assert_eq!(trim_output(&input), dir.join("clip_a_trim2.mp4"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
