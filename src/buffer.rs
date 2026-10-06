use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::audio::HNS_PER_SEC;

/// Encoded MP4 of one segment: in RAM, or a temp file when the buffer is kept on disk.
#[derive(Debug, Clone)]
pub enum SegmentData {
    Memory(Arc<[u8]>),
    File(PathBuf),
}

/// A finished video segment. Times are QPC in 100 ns units (same clock as the audio ring):
/// `start_hns` = first frame, `end_hns` = moment the segment was rotated out.
#[derive(Debug, Clone)]
pub struct SegmentInfo {
    pub data: SegmentData,
    pub start_hns: i64,
    pub end_hns: i64,
}

impl SegmentInfo {
    pub fn duration_secs(&self) -> f64 {
        (self.end_hns - self.start_hns) as f64 / HNS_PER_SEC as f64
    }

    fn delete(&self) {
        if let SegmentData::File(path) = &self.data {
            let _ = fs::remove_file(path);
        }
    }
}

#[derive(Debug, Clone)]
pub struct RingBufferManager {
    inner: Arc<Mutex<RingBufferInner>>,
}

#[derive(Debug)]
struct RingBufferInner {
    segments: VecDeque<SegmentInfo>,
    max_duration_secs: f64,
    /// Bumped by every clear; segments from an older generation are refused.
    generation: u64,
}

impl RingBufferManager {
    pub fn new(max_duration_secs: f64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RingBufferInner {
                segments: VecDeque::new(),
                max_duration_secs,
                generation: 0,
            })),
        }
    }

    /// Updates the target maximum duration of the buffer.
    pub fn update_max_duration(&self, new_duration: f64) {
        self.inner.lock().unwrap().max_duration_secs = new_duration;
    }

    /// (segments, seconds of video, bytes held in RAM) currently in the buffer.
    pub fn stats(&self) -> (usize, f64, usize) {
        let inner = self.inner.lock().unwrap();
        let secs = inner.segments.iter().map(|s| s.duration_secs()).sum::<f64>().max(0.0);
        let bytes = inner
            .segments
            .iter()
            .map(|s| match &s.data {
                SegmentData::Memory(b) => b.len(),
                SegmentData::File(_) => 0,
            })
            .sum();
        (inner.segments.len(), secs, bytes)
    }

    pub fn generation(&self) -> u64 {
        self.inner.lock().unwrap().generation
    }

    /// Adds a completed segment and drops segments no longer needed for the longest clip.
    pub fn add_segment(&self, segment: SegmentInfo, generation: u64) {
        let mut inner = self.inner.lock().unwrap();
        if generation != inner.generation {
            segment.delete();
            return;
        }
        inner.segments.push_back(segment);

        let mut total: f64 = inner.segments.iter().map(|s| s.duration_secs()).sum();
        while let Some(front) = inner.segments.front() {
            if total - front.duration_secs() < inner.max_duration_secs {
                break;
            }
            total -= front.duration_secs();
            front.delete();
            inner.segments.pop_front();
        }
    }

    /// The newest segments that together cover at least `secs` (or everything there is), oldest first.
    pub fn last_segments(&self, secs: f64) -> Vec<SegmentInfo> {
        let inner = self.inner.lock().unwrap();
        let mut covered = 0.0;
        let mut take = 0;
        for seg in inner.segments.iter().rev() {
            if covered >= secs {
                break;
            }
            covered += seg.duration_secs();
            take += 1;
        }
        inner.segments.iter().skip(inner.segments.len() - take).cloned().collect()
    }

    /// Drops every segment (e.g. on shutdown or when the capture source changes).
    pub fn clear_and_delete_all(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.generation += 1;
        for seg in inner.segments.drain(..) {
            seg.delete();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: i64, end: i64) -> SegmentInfo {
        SegmentInfo { data: SegmentData::Memory(Arc::from(vec![0u8; 3])), start_hns: start * HNS_PER_SEC, end_hns: end * HNS_PER_SEC }
    }

    #[test]
    fn keeps_enough_for_longest_clip_and_picks_tail() {
        let ring = RingBufferManager::new(25.0);
        for i in 0..5 {
            ring.add_segment(seg(i * 10, i * 10 + 10), 0);
        }
        // 25 s needs three 10 s segments.
        assert_eq!(ring.stats(), (3, 30.0, 9));
        let tail = ring.last_segments(15.0);
        assert_eq!(tail.iter().map(|s| s.start_hns / HNS_PER_SEC).collect::<Vec<_>>(), [30, 40]);
        assert_eq!(ring.last_segments(999.0).len(), 3);
        ring.clear_and_delete_all();
        ring.add_segment(seg(50, 60), 0); // finished after the clear: stale
        assert_eq!(ring.stats().0, 0);
    }
}
