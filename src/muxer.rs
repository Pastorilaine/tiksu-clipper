//! Everything that runs ffmpeg/ffprobe: clip concat, trimming, thumbnails, durations.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::audio::HNS_PER_SEC;
use crate::buffer::{SegmentData, SegmentInfo};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `ffmpeg.exe` next to our exe wins (portable install), then the copy the app downloaded itself,
/// otherwise whatever is on PATH.
fn tool(name: &str) -> Command {
    let file = format!("{}.exe", name);
    let beside_exe = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join(&file)));
    let downloaded = crate::ffmpeg::tool_dir().map(|dir| dir.join(&file));
    let local = [beside_exe, downloaded].into_iter().flatten().find(|p| p.exists());
    let mut cmd = Command::new(local.unwrap_or_else(|| PathBuf::from(name)));
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

pub fn ffmpeg_available() -> bool {
    tool("ffmpeg").arg("-version").output().is_ok_and(|o| o.status.success())
}

fn run(mut cmd: Command) -> Result<Vec<u8>, String> {
    let output = cmd
        .output()
        .map_err(|e| format!("FFmpegin käynnistys epäonnistui (onko ffmpeg asennettu?): {}", e))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("FFmpeg: {}", stderr.lines().last().unwrap_or_default()))
    }
}

fn seg_path(seg: &SegmentInfo) -> &Path {
    match &seg.data {
        SegmentData::File(path) => path,
        SegmentData::Memory(_) => unreachable!("memory segments are written to files before concat"),
    }
}

/// FFmpeg concat list. Each segment's `duration` is the wall time until the next segment starts,
/// so periods without new frames (static window) are kept as a frozen frame instead of
/// collapsing — the video timeline then matches the wall-clock audio track.
fn concat_list(segments: &[SegmentInfo]) -> String {
    let mut list = String::new();
    for (i, seg) in segments.iter().enumerate() {
        let until = segments.get(i + 1).map_or(seg.end_hns, |next| next.start_hns);
        let duration = (until - seg.start_hns) as f64 / HNS_PER_SEC as f64;
        // Inside single quotes a literal ' is written as '\'' (e.g. a user profile named O'Brien).
        let path = seg_path(seg).to_string_lossy().replace('\\', "/").replace('\'', "'\\''");
        list.push_str(&format!("file '{}'\nduration {:.6}\n", path, duration));
    }
    list
}

/// Concatenates file segments into the final clip without re-encoding the video (stream copy).
/// Each `(wav, title)` becomes an AAC audio track; the first one is the default track.
pub fn concat_segments(segments: &[SegmentInfo], audio: &[(PathBuf, &str)], output_file: &Path) -> Result<(), String> {
    if segments.is_empty() {
        return Err("Puskurissa ei ole vielä kuvaa tallennettavaksi".to_string());
    }
    if let Some(parent) = output_file.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let list_path = seg_path(&segments[0]).with_file_name("concat.txt");
    fs::write(&list_path, concat_list(segments)).map_err(|e| format!("Concat-listan kirjoitus epäonnistui: {}", e))?;

    let mut cmd = tool("ffmpeg");
    cmd.args(["-y", "-f", "concat", "-safe", "0", "-i"]).arg(&list_path);
    for (wav, _) in audio {
        cmd.arg("-i").arg(wav);
    }
    cmd.args(["-map", "0:v:0"]);
    for (i, (_, title)) in audio.iter().enumerate() {
        cmd.args(["-map", &format!("{}:a:0", i + 1)]);
        cmd.arg(format!("-metadata:s:a:{}", i)).arg(format!("title={}", title));
        cmd.arg(format!("-disposition:a:{}", i)).arg(if i == 0 { "default" } else { "0" });
    }
    if !audio.is_empty() {
        cmd.args(["-c:a", "aac", "-b:a", "192k"]);
    }
    cmd.args(["-c:v", "copy", "-movflags", "+faststart"]).arg(output_file);

    let result = run(cmd);
    let _ = fs::remove_file(&list_path);
    result.map(|_| ())
}

/// Cuts `[start, end]` seconds out of a clip into `output`. Re-encodes for a frame-exact cut;
/// falls back to a keyframe-aligned stream copy if this ffmpeg has no libx264.
pub fn trim(input: &Path, start: f64, end: f64, output: &Path) -> Result<(), String> {
    let cut = |video: &[&str]| {
        let mut cmd = tool("ffmpeg");
        cmd.args(["-y", "-ss", &format!("{:.3}", start), "-i"])
            .arg(input)
            .args(["-t", &format!("{:.3}", (end - start).max(0.1)), "-map", "0"])
            .args(video)
            .args(["-c:a", "copy", "-movflags", "+faststart"])
            .arg(output);
        run(cmd)
    };
    cut(&["-c:v", "libx264", "-preset", "veryfast", "-crf", "18"])
        .or_else(|_| cut(&["-c:v", "copy"]))
        .map(|_| ())
}

/// One frame at `at` seconds, letterboxed to exactly `w`×`h` RGBA.
pub fn thumbnail(input: &Path, at: f64, w: u32, h: u32) -> Result<Vec<u8>, String> {
    let filter = format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2"
    );
    let mut cmd = tool("ffmpeg");
    cmd.args(["-v", "error", "-ss", &format!("{:.3}", at.max(0.0)), "-i"])
        .arg(input)
        .args(["-frames:v", "1", "-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgba", "-"]);
    let rgba = run(cmd)?;
    if rgba.len() == (w * h * 4) as usize {
        Ok(rgba)
    } else if at > 0.5 {
        // Past the last frame (audio can run a moment longer than video): step back.
        thumbnail(input, at - 1.0, w, h)
    } else {
        Err("ei kuvaa tässä kohdassa".to_string())
    }
}

/// Clip length in seconds.
pub fn duration(input: &Path) -> Option<f64> {
    let mut cmd = tool("ffprobe");
    cmd.args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"]).arg(input);
    String::from_utf8_lossy(&run(cmd).ok()?).trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_span_until_next_segment_start() {
        let seg = |name: &str, start: i64, end: i64| SegmentInfo {
            data: SegmentData::File(name.into()),
            start_hns: start * HNS_PER_SEC,
            end_hns: end * HNS_PER_SEC,
        };
        // Segment b had no frames for 2 s after a's rotation: a's frozen frame covers the gap.
        let list = concat_list(&[seg("a.mp4", 0, 10), seg("b.mp4", 12, 20)]);
        assert_eq!(list, "file 'a.mp4'\nduration 12.000000\nfile 'b.mp4'\nduration 8.000000\n");
    }

    #[test]
    fn apostrophe_in_path_is_escaped() {
        let seg = SegmentInfo { data: SegmentData::File(r"C:\Users\O'Brien\a.mp4".into()), start_hns: 0, end_hns: HNS_PER_SEC };
        assert_eq!(concat_list(&[seg]), "file 'C:/Users/O'\\''Brien/a.mp4'\nduration 1.000000\n");
    }
}

#[cfg(test)]
mod real_clip_tests {
    use super::*;

    /// Manual, after the capture e2e test: `cargo test real_clip -- --ignored`
    #[test]
    #[ignore]
    fn real_clip_trim_thumbnail_duration() {
        let dir = std::env::temp_dir().join("silly_borg_e2e");
        let clip = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).find(|p| !p.to_string_lossy().contains("_trim")).unwrap();
        let d = duration(&clip).unwrap();
        assert!(d > 3.0, "{}", d);
        assert_eq!(thumbnail(&clip, 1.0, 160, 90).unwrap().len(), 160 * 90 * 4);
        let out = dir.join("cut_trim.mp4");
        trim(&clip, 1.0, 3.5, &out).unwrap();
        let cut = duration(&out).unwrap();
        assert!((cut - 2.5).abs() < 0.2, "{}", cut);
    }
}
