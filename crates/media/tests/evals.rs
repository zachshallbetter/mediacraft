//! Adversarial evaluations, diagnostics, and fallback verification for `craft-media`.

use craft_media::{
    AudioBuffer, AudioBufferRef, AudioStreamDescriptor, ChannelLayout, Packet,
    PacketTimelineValidator, PixelFormat, Rational, SampleFormat, TimelineAnomaly,
    TimelineFallbackPolicy, VideoStreamDescriptor,
};

#[test]
fn eval_adversarial_rational_math() {
    // Zero denominator must not panic or cause SIGFPE
    let zero_den = Rational::new(100, 0);
    assert_eq!(zero_den.den, 1);
    assert_eq!(zero_den.as_f64(), 100.0);

    // Rescale with zero target numerator or denominator returns 0 without panic
    let r1 = Rational::new(1, 24);
    let r_bad = Rational::new(0, 100);
    assert_eq!(r1.rescale(1000, r_bad), 0);

    // Extreme timestamps rescale safely without overflow panic
    let huge_pts = i64::MAX / 2;
    let r_24 = Rational::new(1, 24);
    let r_48k = Rational::new(1, 48000);
    let rescaled = r_24.rescale(huge_pts, r_48k);
    assert!(rescaled > 0);

    // Negative timestamps preserve sign accurately (audio lead-in / preroll)
    let neg_pts = -4800i64; // 100ms at 48kHz
    let rescaled_neg = r_48k.rescale(neg_pts, Rational::new(1, 1000));
    assert_eq!(rescaled_neg, -100);
}

#[test]
fn eval_audio_stream_descriptor_diagnostics() {
    // Valid stereo 48kHz audio
    let valid_audio = AudioStreamDescriptor {
        stream_index: 0,
        codec: "flac".to_string(),
        sample_rate: 48000,
        channels: 2,
        channel_layout: ChannelLayout::Stereo,
        sample_format: SampleFormat::S16Planar,
        bitrate_bps: Some(1536000),
        extra_data: Vec::new(),
    };
    let report = valid_audio.diagnose();
    assert!(report.is_valid);
    assert!(report.errors.is_empty());
    assert!(report.warnings.is_empty());

    // Adversarial: 0 channels and 0 Hz sample rate
    let broken_audio = AudioStreamDescriptor {
        stream_index: 1,
        codec: "pcm".to_string(),
        sample_rate: 0,
        channels: 0,
        channel_layout: ChannelLayout::Surround51, // Layout says 6, channels says 0!
        sample_format: SampleFormat::F32Planar,
        bitrate_bps: None,
        extra_data: Vec::new(),
    };
    let broken_report = broken_audio.diagnose();
    assert!(!broken_report.is_valid);
    assert!(
        broken_report
            .errors
            .iter()
            .any(|e| e.contains("channel count"))
    );
    assert!(
        broken_report
            .errors
            .iter()
            .any(|e| e.contains("sample rate"))
    );
    assert!(
        broken_report
            .warnings
            .iter()
            .any(|w| w.contains("mismatch"))
    );
}

#[test]
fn eval_video_stream_descriptor_diagnostics() {
    // Valid 4K 60fps video
    let valid_video = VideoStreamDescriptor {
        stream_index: 0,
        codec: "h264".to_string(),
        width: 3840,
        height: 2160,
        pixel_format: PixelFormat::Yuv420p,
        frame_rate: Rational::new(60, 1),
        bitrate_bps: Some(25_000_000),
        extra_data: Vec::new(),
    };
    let report = valid_video.diagnose();
    assert!(report.is_valid);
    assert!(report.errors.is_empty());

    // Hostile: 0x0 video dimensions and zero frame rate
    let invalid_video = VideoStreamDescriptor {
        stream_index: 0,
        codec: "h264".to_string(),
        width: 0,
        height: 0,
        pixel_format: PixelFormat::Rgba,
        frame_rate: Rational::new(0, 0),
        bitrate_bps: None,
        extra_data: Vec::new(),
    };
    let inv_report = invalid_video.diagnose();
    assert!(!inv_report.is_valid);
    assert!(
        inv_report
            .errors
            .iter()
            .any(|e| e.contains("invalid dimensions"))
    );
    assert!(inv_report.errors.iter().any(|e| e.contains("frame rate")));
}

#[test]
fn eval_packet_timeline_validator_and_monotonic_fallback() {
    let mut validator = PacketTimelineValidator::new(TimelineFallbackPolicy::ClampMonotonic);
    let timebase = Rational::new(1, 1000);

    // Packet 0: Normal PTS 0, DTS 0
    let mut p0 = Packet::new(0, 0, 0, 40, timebase, true, vec![0x01, 0x02]);
    assert_eq!(validator.process_packet(&mut p0), None);

    // Packet 1: Normal PTS 80, DTS 40 (B-frame reordering)
    let mut p1 = Packet::new(0, 80, 40, 40, timebase, false, vec![0x03]);
    assert_eq!(validator.process_packet(&mut p1), None);

    // Packet 2: Hostile backwards DTS (e.g. corrupted stream: DTS drops to 20!)
    let mut p2 = Packet::new(0, 50, 20, 40, timebase, false, vec![0x04]);
    let anomaly = validator.process_packet(&mut p2);
    assert!(matches!(
        anomaly,
        Some(TimelineAnomaly::BackwardsDecodingTimestamp {
            previous_dts: 40,
            current_dts: 20
        })
    ));
    // Verify fallback policy clamped DTS monotonic
    assert_eq!(p2.time.dts, 41);
    assert!(p2.time.pts >= p2.time.dts);

    // Packet 3: Causal violation (PTS < DTS: PTS 30, DTS 50)
    let mut p3 = Packet::new(0, 30, 50, 40, timebase, false, vec![0x05]);
    let anomaly3 = validator.process_packet(&mut p3);
    assert!(matches!(
        anomaly3,
        Some(TimelineAnomaly::PresentationPrecedesDecoding { pts: 30, dts: 50 })
    ));
    // Verify fallback clamped PTS to DTS
    assert_eq!(p3.time.pts, 50);

    let summary = validator.summary();
    assert_eq!(summary.packet_count, 4);
    assert_eq!(summary.out_of_order_count, 1);
}

#[test]
fn eval_audio_buffer_bounds_and_silence_fallback() {
    let silence = AudioBuffer::silence(6, 1024, 48000);
    assert_eq!(silence.channels(), 6);
    assert_eq!(silence.frames(), 1024);
    assert_eq!(silence.sample_rate(), 48000);

    for ch in 0..silence.channels() {
        let channel_data = silence.channel(ch).unwrap();
        assert_eq!(channel_data.len(), 1024);
        assert!(channel_data.iter().all(|&s| s == 0.0));
    }

    // Access beyond channel count returns None safely without panic
    assert!(silence.channel(6).is_none());
}
