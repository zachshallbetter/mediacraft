//! Diagnostic reporting and structural analysis for ISO BMFF / QuickTime files.

use serde::{Deserialize, Serialize};

use crate::demux::{Mp4File, Track, TrackKind};

/// Diagnostic report for a single track in an ISO BMFF / QuickTime container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackDiagnosticReport {
    pub track_id: u32,
    pub kind: String,
    pub codec: String,
    pub timescale: u32,
    pub duration_seconds: f64,
    pub sample_count: usize,
    pub sync_sample_count: usize,
    pub max_keyframe_interval_samples: usize,
    pub has_b_frames: bool,
    pub has_edit_list: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Comprehensive diagnostic report for an ISO BMFF / QuickTime container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IsobmffDiagnosticReport {
    pub major_brand: String,
    pub compatible_brands: Vec<String>,
    pub is_quicktime: bool,
    pub is_fragmented: bool,
    pub is_faststart: bool,
    pub moov_offset: u64,
    pub timescale: u32,
    pub duration_seconds: f64,
    pub track_count: usize,
    pub tracks: Vec<TrackDiagnosticReport>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl Mp4File {
    /// Evaluates container structure, stream sync, faststart placement, and keyframe cadences.
    pub fn diagnose(&self) -> IsobmffDiagnosticReport {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        if self.timescale == 0 {
            errors.push("movie header timescale cannot be zero".to_string());
        }

        let duration_seconds = if self.timescale > 0 {
            (self.duration as f64) / (self.timescale as f64)
        } else {
            0.0
        };

        // Faststart heuristic: in progressive files, if moov appears near the file header (< 64KB)
        // rather than at the end of large media payloads, web progressive playback is immediate.
        let is_faststart = self.moov_offset < 65536;
        if !is_faststart && !self.fragmented {
            warnings.push(format!(
                "moov box located at byte offset {} (non-faststart layout requires full download before playback)",
                self.moov_offset
            ));
        }

        let mut tracks = Vec::new();
        for t in &self.tracks {
            let trk_report = diagnose_track(t);
            tracks.push(trk_report);
        }

        if tracks.is_empty() {
            warnings.push("container contains no active tracks".to_string());
        }

        IsobmffDiagnosticReport {
            major_brand: self.major_brand.to_string(),
            compatible_brands: self
                .compatible_brands
                .iter()
                .map(|b| b.to_string())
                .collect(),
            is_quicktime: self.is_quicktime,
            is_fragmented: self.fragmented,
            is_faststart,
            moov_offset: self.moov_offset,
            timescale: self.timescale,
            duration_seconds,
            track_count: self.tracks.len(),
            tracks,
            warnings,
            errors,
        }
    }
}

fn diagnose_track(track: &Track) -> TrackDiagnosticReport {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    if track.timescale == 0 {
        errors.push("track media timescale cannot be zero".to_string());
    }

    let duration_seconds = if track.timescale > 0 {
        (track.duration as f64) / (track.timescale as f64)
    } else {
        0.0
    };

    let kind_str = match track.kind {
        TrackKind::Video => "video",
        TrackKind::Audio => "audio",
        TrackKind::Timecode => "timecode",
        TrackKind::Subtitle => "subtitle",
        TrackKind::Other => "other",
    };

    let codec_str = track
        .codec()
        .map(|c| format!("{:?}", c))
        .unwrap_or_else(|| "unknown".to_string());

    let mut sync_sample_count: usize = 0;
    let mut max_keyframe_interval_samples: usize = 0;
    let mut current_interval: usize = 0;
    let mut has_b_frames = false;
    let mut last_dts: Option<i64> = None;

    for s in &track.samples {
        if s.is_sync {
            sync_sample_count = sync_sample_count.saturating_add(1);
            if current_interval > max_keyframe_interval_samples {
                max_keyframe_interval_samples = current_interval;
            }
            current_interval = 0;
        } else {
            current_interval = current_interval.saturating_add(1);
        }

        if s.pts != s.dts {
            has_b_frames = true;
        }

        if s.pts < s.dts {
            warnings.push(format!(
                "causal violation: PTS {} is less than DTS {}",
                s.pts, s.dts
            ));
        }

        if let Some(prev) = last_dts
            && s.dts < prev
        {
            warnings.push(format!(
                "non-monotonic DTS sequence: {} follows {}",
                s.dts, prev
            ));
        }
        last_dts = Some(s.dts);
    }

    if current_interval > max_keyframe_interval_samples {
        max_keyframe_interval_samples = current_interval;
    }

    if track.kind == TrackKind::Video && sync_sample_count == 0 && !track.samples.is_empty() {
        warnings.push("video track contains zero sync/keyframe samples".to_string());
    }

    let has_edit_list = !track.edits.is_empty();

    TrackDiagnosticReport {
        track_id: track.id,
        kind: kind_str.to_string(),
        codec: codec_str,
        timescale: track.timescale,
        duration_seconds,
        sample_count: track.samples.len(),
        sync_sample_count,
        max_keyframe_interval_samples,
        has_b_frames,
        has_edit_list,
        warnings,
        errors,
    }
}
