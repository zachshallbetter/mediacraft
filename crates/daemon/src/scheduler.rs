//! Multi-Stream Synchronization Scheduler
//!
//! Coordinates the interleaving of disparate media streams (e.g. 23.976 fps video
//! and 48 kHz audio) into strict presentation and decode timestamp ordering for container multiplexers.
//! Tracks timeline drift and prevents audio/video buffer starvation.

use mediacraft_protocol::stream::{MediaPacket, Rational};
use std::collections::VecDeque;

/// Configuration for an individual stream tracked by the scheduler.
#[derive(Debug, Clone)]
pub struct StreamConfig {
    pub stream_index: usize,
    pub timebase: Rational,
    pub is_video: bool,
}

/// Internal queue for buffering packets of a single stream.
#[derive(Debug, Default)]
struct StreamQueue {
    packets: VecDeque<MediaPacket>,
    is_eos: bool,
    last_dts: Option<i64>,
    last_pts: Option<i64>,
    total_packets_emitted: usize,
    last_emitted_pts_sec: f64,
}

/// Report summarizing timeline alignment and maximum observed skew.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncReport {
    pub total_packets_scheduled: usize,
    pub max_skew_ms: f64,
    pub video_duration_sec: f64,
    pub audio_duration_sec: f64,
    pub is_synchronized: bool,
}

/// Coordinates multiplexing across audio and video streams.
pub struct StreamScheduler {
    streams: Vec<StreamQueue>,
    configs: Vec<StreamConfig>,
    max_observed_skew_ms: f64,
    total_scheduled: usize,
}

impl StreamScheduler {
    /// Creates a scheduler for the declared stream configurations.
    pub fn new(configs: Vec<StreamConfig>) -> Self {
        let stream_count = configs.len();
        let mut streams = Vec::with_capacity(stream_count);
        for _ in 0..stream_count {
            streams.push(StreamQueue::default());
        }

        Self {
            streams,
            configs,
            max_observed_skew_ms: 0.0,
            total_scheduled: 0,
        }
    }

    /// Enqueues a packet from an encoder or demuxer.
    pub fn push_packet(&mut self, packet: MediaPacket) {
        let idx = packet.stream_index;
        if let Some(queue) = self.streams.get_mut(idx) {
            queue.last_dts = Some(packet.dts);
            queue.last_pts = Some(packet.pts);
            queue.packets.push_back(packet);
        }
    }

    /// Signals end-of-stream for a specific stream index.
    pub fn signal_eos(&mut self, stream_index: usize) {
        if let Some(queue) = self.streams.get_mut(stream_index) {
            queue.is_eos = true;
        }
    }

    /// Checks if all registered streams have completed and emptied their buffers.
    pub fn is_finished(&self) -> bool {
        self.streams
            .iter()
            .all(|q| q.is_eos && q.packets.is_empty())
    }

    /// Pops the next packet that should be written to the container muxer.
    /// Selects the stream with the lowest decode timestamp (`dts`) in seconds.
    pub fn pop_next_packet(&mut self) -> Option<MediaPacket> {
        let mut candidate_idx: Option<usize> = None;
        let mut min_dts_sec = f64::INFINITY;

        // Ensure every non-EOS stream has at least one buffered packet before deciding,
        // unless all active streams with buffered data have passed the threshold.
        let mut has_undecided_stream = false;
        for (idx, q) in self.streams.iter().enumerate() {
            if !q.is_eos && q.packets.is_empty() {
                has_undecided_stream = true;
            }
            if let Some(front) = q.packets.front() {
                let dts_sec = front.dts_seconds();
                if dts_sec < min_dts_sec {
                    min_dts_sec = dts_sec;
                    candidate_idx = Some(idx);
                }
            }
        }

        // If an active stream has zero packets and has not reached EOS, wait for input
        // to avoid interleaving out of DTS order (unless we are flushing).
        if has_undecided_stream && !self.all_streams_have_data_or_eos() {
            return None;
        }

        let idx = candidate_idx?;
        let queue = self.streams.get_mut(idx)?;
        let pkt = queue.packets.pop_front()?;

        queue.total_packets_emitted += 1;
        queue.last_emitted_pts_sec = pkt.pts_seconds();
        self.total_scheduled += 1;

        self.update_skew();
        Some(pkt)
    }

    fn all_streams_have_data_or_eos(&self) -> bool {
        self.streams
            .iter()
            .all(|q| q.is_eos || !q.packets.is_empty())
    }

    fn update_skew(&mut self) {
        let mut video_pts_sec: Option<f64> = None;
        let mut audio_pts_sec: Option<f64> = None;

        for (idx, q) in self.streams.iter().enumerate() {
            if let Some(cfg) = self.configs.get(idx) {
                if cfg.is_video {
                    video_pts_sec = Some(q.last_emitted_pts_sec);
                } else {
                    audio_pts_sec = Some(q.last_emitted_pts_sec);
                }
            }
        }

        if let (Some(v), Some(a)) = (video_pts_sec, audio_pts_sec) {
            let skew_ms = (v - a).abs() * 1000.0;
            if skew_ms > self.max_observed_skew_ms {
                self.max_observed_skew_ms = skew_ms;
            }
        }
    }

    /// Emits a final synchronization and timing report.
    pub fn finish_report(&self) -> SyncReport {
        let mut video_dur = 0.0;
        let mut audio_dur = 0.0;

        for (idx, q) in self.streams.iter().enumerate() {
            if let Some(cfg) = self.configs.get(idx) {
                if cfg.is_video {
                    video_dur = q.last_emitted_pts_sec;
                } else {
                    audio_dur = q.last_emitted_pts_sec;
                }
            }
        }

        SyncReport {
            total_packets_scheduled: self.total_scheduled,
            max_skew_ms: self.max_observed_skew_ms,
            video_duration_sec: video_dur,
            audio_duration_sec: audio_dur,
            // Industry standard: maximum interleaving skew under 1000ms is well-synchronized
            is_synchronized: self.max_observed_skew_ms < 1000.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_scheduler_interleaving() {
        let configs = vec![
            StreamConfig {
                stream_index: 0,
                timebase: Rational::FPS_24,
                is_video: true,
            },
            StreamConfig {
                stream_index: 1,
                timebase: Rational::HZ_48000,
                is_video: false,
            },
        ];

        let mut scheduler = StreamScheduler::new(configs);

        // Feed 24 video frames (1.0 sec at 24 fps)
        for i in 0..24 {
            scheduler.push_packet(MediaPacket {
                stream_index: 0,
                data: vec![0x00, 0x00, 0x01, 0x65],
                pts: i,
                dts: i,
                duration: 1,
                timebase: Rational::FPS_24,
                is_keyframe: i == 0,
            });
        }
        scheduler.signal_eos(0);

        // Feed 47 AAC audio packets (1024 samples each at 48kHz = ~1.0 sec)
        for i in 0..47 {
            let pts = (i as i64) * 1024;
            scheduler.push_packet(MediaPacket {
                stream_index: 1,
                data: vec![0xFF, 0xF1],
                pts,
                dts: pts,
                duration: 1024,
                timebase: Rational::HZ_48000,
                is_keyframe: true,
            });
        }
        scheduler.signal_eos(1);

        // Interleave packets
        let mut interleaved = Vec::new();
        while let Some(pkt) = scheduler.pop_next_packet() {
            interleaved.push(pkt);
        }

        assert_eq!(interleaved.len(), 24 + 47);
        assert!(scheduler.is_finished());

        let report = scheduler.finish_report();
        assert!(report.is_synchronized);
        assert!(
            report.max_skew_ms < 150.0,
            "Max skew was: {}",
            report.max_skew_ms
        );
    }
}
