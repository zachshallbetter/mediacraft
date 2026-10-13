//! Media stream descriptors, codec parameters, and compressed packets.

use serde::{Deserialize, Serialize};

use crate::audio::{ChannelLayout, SampleFormat};
use crate::time::{MediaTime, Rational};
use crate::video::PixelFormat;

/// Classification of media elementary stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StreamKind {
    Audio,
    Video,
    Data,
    Subtitle,
}

/// Detailed descriptor for an audio elementary stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioStreamDescriptor {
    pub stream_index: usize,
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub channel_layout: ChannelLayout,
    pub sample_format: SampleFormat,
    pub bitrate_bps: Option<u32>,
    pub extra_data: Vec<u8>,
}

/// Detailed descriptor for a video elementary stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoStreamDescriptor {
    pub stream_index: usize,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
    pub frame_rate: Rational,
    pub bitrate_bps: Option<u32>,
    pub extra_data: Vec<u8>,
}

impl AudioStreamDescriptor {
    /// Inspects the audio descriptor and reports anomalies, mismatches, or errors.
    pub fn diagnose(&self) -> StreamDiagnosticReport {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        if self.channels == 0 {
            errors.push("channel count cannot be 0".to_string());
        } else if self.channels > 64 {
            warnings.push(format!("unusually high channel count: {}", self.channels));
        }

        if self.channel_layout.channels() != self.channels as usize {
            warnings.push(format!(
                "channel count mismatch: declared {} but layout defines {}",
                self.channels,
                self.channel_layout.channels()
            ));
        }

        if self.sample_rate == 0 {
            errors.push("sample rate cannot be 0".to_string());
        } else if self.sample_rate < 8000 || self.sample_rate > 768_000 {
            warnings.push(format!(
                "unconventional sample rate: {} Hz (typical 44100-192000)",
                self.sample_rate
            ));
        }

        StreamDiagnosticReport {
            is_valid: errors.is_empty(),
            warnings,
            errors,
        }
    }
}

impl VideoStreamDescriptor {
    /// Inspects the video descriptor and reports anomalies, mismatches, or errors.
    pub fn diagnose(&self) -> StreamDiagnosticReport {
        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        if self.width == 0 || self.height == 0 {
            errors.push(format!(
                "invalid dimensions: {}x{}",
                self.width, self.height
            ));
        } else if self.width > 16384 || self.height > 16384 {
            warnings.push(format!(
                "extreme video resolution: {}x{} exceeds standard broadcast caps",
                self.width, self.height
            ));
        }

        if self.frame_rate.den == 0 || self.frame_rate.num == 0 {
            errors
                .push("invalid frame rate: denominator and numerator must be non-zero".to_string());
        } else {
            let fps = self.frame_rate.as_f64();
            if fps > 480.0 {
                warnings.push(format!("abnormally high frame rate: {:.2} fps", fps));
            }
        }

        StreamDiagnosticReport {
            is_valid: errors.is_empty(),
            warnings,
            errors,
        }
    }
}

/// Codec identification and initialization parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodecParameters {
    pub fourcc: [u8; 4],
    pub extra_data: Vec<u8>,
}

/// Compressed elementary stream packet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Packet {
    pub stream_index: usize,
    pub time: MediaTime,
    pub is_keyframe: bool,
    pub data: Vec<u8>,
}

impl Packet {
    pub fn new(
        stream_index: usize,
        pts: i64,
        dts: i64,
        duration: u64,
        timebase: Rational,
        is_keyframe: bool,
        data: Vec<u8>,
    ) -> Self {
        Self {
            stream_index,
            time: MediaTime::with_dts(pts, dts, duration, timebase),
            is_keyframe,
            data,
        }
    }
}

/// Diagnostic report for stream configuration validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamDiagnosticReport {
    pub is_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Policy for handling packet timeline anomalies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TimelineFallbackPolicy {
    /// Report anomalies via return status without modifying the packet.
    #[default]
    ReportOnly,
    /// Enforce monotonic DTS order by advancing DTS if it steps backwards, and clamping PTS >= DTS.
    ClampMonotonic,
}

/// Timeline anomaly detected in a packet stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimelineAnomaly {
    /// Presentation timestamp precedes decoding timestamp (causal ordering violation).
    PresentationPrecedesDecoding { pts: i64, dts: i64 },
    /// Decoding timestamp stepped backwards compared to the previous packet.
    BackwardsDecodingTimestamp { previous_dts: i64, current_dts: i64 },
    /// Non-positive packet duration in active stream.
    ZeroDuration,
}

/// Stream packet timeline validator with monotonic fallback correction and logging.
#[derive(Debug, Clone)]
pub struct PacketTimelineValidator {
    policy: TimelineFallbackPolicy,
    last_dts: Option<i64>,
    packet_count: usize,
    out_of_order_count: usize,
    negative_duration_count: usize,
}

impl PacketTimelineValidator {
    pub fn new(policy: TimelineFallbackPolicy) -> Self {
        Self {
            policy,
            last_dts: None,
            packet_count: 0,
            out_of_order_count: 0,
            negative_duration_count: 0,
        }
    }

    /// Evaluates and optionally corrects a packet timeline, logging anomalies.
    pub fn process_packet(&mut self, packet: &mut Packet) -> Option<TimelineAnomaly> {
        self.packet_count = self.packet_count.saturating_add(1);
        let mut anomaly = None;

        if packet.time.pts < packet.time.dts {
            tracing::warn!(
                stream_index = packet.stream_index,
                pts = packet.time.pts,
                dts = packet.time.dts,
                "presentation timestamp precedes decoding timestamp"
            );
            anomaly = Some(TimelineAnomaly::PresentationPrecedesDecoding {
                pts: packet.time.pts,
                dts: packet.time.dts,
            });

            if self.policy == TimelineFallbackPolicy::ClampMonotonic {
                packet.time.pts = packet.time.dts;
            }
        }

        if let Some(prev_dts) = self.last_dts
            && packet.time.dts < prev_dts
        {
            self.out_of_order_count = self.out_of_order_count.saturating_add(1);
            tracing::warn!(
                stream_index = packet.stream_index,
                prev_dts = prev_dts,
                current_dts = packet.time.dts,
                "backwards decoding timestamp detected"
            );
            anomaly = Some(TimelineAnomaly::BackwardsDecodingTimestamp {
                previous_dts: prev_dts,
                current_dts: packet.time.dts,
            });

            if self.policy == TimelineFallbackPolicy::ClampMonotonic {
                let fallback_dts = prev_dts.saturating_add(1);
                packet.time.dts = fallback_dts;
                if packet.time.pts < fallback_dts {
                    packet.time.pts = fallback_dts;
                }
            }
        }

        if packet.time.duration == 0 {
            self.negative_duration_count = self.negative_duration_count.saturating_add(1);
            if anomaly.is_none() {
                anomaly = Some(TimelineAnomaly::ZeroDuration);
            }
        }

        self.last_dts = Some(packet.time.dts);
        anomaly
    }

    pub fn packet_count(&self) -> usize {
        self.packet_count
    }

    pub fn out_of_order_count(&self) -> usize {
        self.out_of_order_count
    }

    pub fn summary(&self) -> TimelineSummary {
        TimelineSummary {
            packet_count: self.packet_count,
            out_of_order_count: self.out_of_order_count,
            negative_duration_count: self.negative_duration_count,
        }
    }
}

/// Aggregated summary of timeline validation across packets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineSummary {
    pub packet_count: usize,
    pub out_of_order_count: usize,
    pub negative_duration_count: usize,
}
