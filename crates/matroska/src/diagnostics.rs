//! Diagnostic reporting and container integrity analysis for Matroska (MKV) and WebM files.

use serde::{Deserialize, Serialize};

use crate::demux::MkvFile;
use crate::track::TrackKind;

/// Diagnostic report for an individual track within a Matroska / WebM container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatroskaTrackReport {
    pub track_number: u64,
    pub track_uid: u64,
    pub kind: String,
    pub codec_id: String,
    pub language: String,
    pub sample_count: usize,
    pub sync_sample_count: usize,
    pub has_lacing: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Comprehensive diagnostic report for a Matroska (MKV) or WebM container.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatroskaDiagnosticReport {
    pub doc_type: String,
    pub doc_type_version: u64,
    pub is_live: bool,
    pub timestamp_scale_ns: u64,
    pub duration_seconds: Option<f64>,
    pub muxing_app: String,
    pub writing_app: String,
    pub track_count: usize,
    pub cluster_count: usize,
    pub cue_count: usize,
    pub chapter_count: usize,
    pub attachment_count: usize,
    pub tracks: Vec<MatroskaTrackReport>,
    pub crc_checked: u32,
    pub crc_error_count: usize,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

impl MkvFile {
    /// Inspects the parsed Matroska/WebM file and compiles an empirical diagnostic report.
    pub fn diagnose(&self) -> MatroskaDiagnosticReport {
        let mut warnings = self.warnings.clone();
        let mut errors = Vec::new();

        if self.doc_type != "matroska" && self.doc_type != "webm" {
            warnings.push(format!("unconventional EBML DocType: '{}'", self.doc_type));
        }

        if self.info.timestamp_scale == 0 {
            errors.push("timestamp scale cannot be 0 ns".to_string());
        }

        let duration_seconds = self
            .info
            .duration
            .map(|d| (d * self.info.timestamp_scale as f64) / 1_000_000_000.0);

        if self.cues.is_empty() && !self.live && self.clusters.len() > 1 {
            warnings.push(
                "container has no Cues seek index; seek operations require sequential cluster scanning"
                    .to_string(),
            );
        }

        if !self.crc_errors.is_empty() {
            warnings.push(format!(
                "detected {} CRC-32 checksum failures across container elements",
                self.crc_errors.len()
            ));
        }

        let mut tracks = Vec::new();
        for t in &self.tracks {
            let mut trk_warnings = Vec::new();
            let mut trk_errors = Vec::new();

            if t.number == 0 {
                trk_errors.push("track number cannot be 0".to_string());
            }

            let kind_str = match t.kind {
                TrackKind::Video => "video",
                TrackKind::Audio => "audio",
                TrackKind::Subtitle => "subtitle",
                TrackKind::Complex => "complex",
                TrackKind::Logo => "logo",
                TrackKind::Buttons => "buttons",
                TrackKind::Control => "control",
                TrackKind::Metadata => "metadata",
                TrackKind::Other(_) => "other",
            };

            let mut sync_sample_count: usize = 0;
            let mut has_lacing = false;
            for s in &t.samples {
                if s.keyframe {
                    sync_sample_count = sync_sample_count.saturating_add(1);
                }
                if s.lace > 0 {
                    has_lacing = true;
                }
            }

            if t.kind == TrackKind::Video && sync_sample_count == 0 && !t.samples.is_empty() {
                trk_warnings.push("video track contains no keyframes".to_string());
            }

            tracks.push(MatroskaTrackReport {
                track_number: t.number,
                track_uid: t.uid,
                kind: kind_str.to_string(),
                codec_id: t.codec_id.clone(),
                language: t.language.clone(),
                sample_count: t.samples.len(),
                sync_sample_count,
                has_lacing,
                warnings: trk_warnings,
                errors: trk_errors,
            });
        }

        MatroskaDiagnosticReport {
            doc_type: self.doc_type.clone(),
            doc_type_version: self.doc_type_version,
            is_live: self.live,
            timestamp_scale_ns: self.info.timestamp_scale,
            duration_seconds,
            muxing_app: self.info.muxing_app.clone(),
            writing_app: self.info.writing_app.clone(),
            track_count: self.tracks.len(),
            cluster_count: self.clusters.len(),
            cue_count: self.cues.len(),
            chapter_count: self.chapters.len(),
            attachment_count: self.attachments.len(),
            tracks,
            crc_checked: self.crc_checked,
            crc_error_count: self.crc_errors.len(),
            warnings,
            errors,
        }
    }
}
