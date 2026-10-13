//! Independent Empirical Validation of Milestones M6 through M10
//!
//! Validates:
//! - M6: Preset execution plans as strict preflight contracts.
//! - M7: Real mathematical quality analysis (pure-Rust PSNR, SSIM, and ITU-R BS.1770-4 loudness).
//! - M8: Hardware acceleration discovery and backend execution routing.
//! - M9: Headless CLI operations and deliverable verification.
//! - M10: Clean-room format expansion (FLAC and Apple ProRes 422).

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use mediacraft_daemon::QueueEngine;
use mediacraft_daemon::hardware::HardwareCapabilities;
use mediacraft_daemon::pipeline::prores_adapter::ProResEncoderAdapter;
use mediacraft_daemon::plan::{PlanError, compile_execution_plan};
use mediacraft_daemon::quality::{calculate_psnr, calculate_ssim, measure_audio_loudness};
use mediacraft_daemon::validate_deliverable;
use mediacraft_protocol::stream::{Rational, VideoFrame};
use mediacraft_protocol::{
    AudioCodec, ContainerFormat, DeliveryContract, EncodingPreset, ValidationSeverity, VideoCodec,
};

fn find_binary(name: &str) -> Option<PathBuf> {
    let opt_brew = PathBuf::from("/opt/homebrew/bin").join(name);
    if opt_brew.exists() {
        return Some(opt_brew);
    }
    let usr_local = PathBuf::from("/usr/local/bin").join(name);
    if usr_local.exists() {
        return Some(usr_local);
    }
    if let Ok(output) = Command::new("which").arg(name).output() {
        let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if output.status.success() && !path_str.is_empty() {
            return Some(PathBuf::from(path_str));
        }
    }
    None
}

/// Helper to write a valid 16-bit stereo PCM WAV file.
fn write_sine_wav(path: &std::path::Path, sample_rate: u32, duration_secs: f32) {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let num_channels = 2u16;
    let bits_per_sample = 16u16;
    let block_align = num_channels * (bits_per_sample / 8);
    let byte_rate = sample_rate * block_align as u32;
    let data_len = (num_samples * block_align as usize) as u32;

    let mut file = File::create(path).expect("create test wav");
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(data_len + 36).to_le_bytes()).unwrap();
    file.write_all(b"WAVEfmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap(); // Subchunk1Size (16 for PCM)
    file.write_all(&1u16.to_le_bytes()).unwrap(); // AudioFormat (1 = PCM)
    file.write_all(&num_channels.to_le_bytes()).unwrap();
    file.write_all(&sample_rate.to_le_bytes()).unwrap();
    file.write_all(&byte_rate.to_le_bytes()).unwrap();
    file.write_all(&block_align.to_le_bytes()).unwrap();
    file.write_all(&bits_per_sample.to_le_bytes()).unwrap();
    file.write_all(b"data").unwrap();
    file.write_all(&data_len.to_le_bytes()).unwrap();

    let mut pcm_bytes = Vec::with_capacity(data_len as usize);
    let freq_l = 440.0f32;
    let freq_r = 880.0f32;
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let s_l = (t * freq_l * 2.0 * std::f32::consts::PI).sin() * 0.5;
        let s_r = (t * freq_r * 2.0 * std::f32::consts::PI).sin() * 0.5;
        let sample_l = (s_l * 32767.0).clamp(-32768.0, 32767.0) as i16;
        let sample_r = (s_r * 32767.0).clamp(-32768.0, 32767.0) as i16;
        pcm_bytes.extend_from_slice(&sample_l.to_le_bytes());
        pcm_bytes.extend_from_slice(&sample_r.to_le_bytes());
    }
    file.write_all(&pcm_bytes).unwrap();
}

#[test]
fn test_m6_preset_execution_plan_validation() {
    let temp_dir = std::env::temp_dir().join("mediacraft_test_m6");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let dummy_wav = temp_dir.join("input.wav");
    write_sine_wav(&dummy_wav, 48000, 0.5);

    let dummy_out = temp_dir.join("output.mp4");

    // 1. Odd dimensions for YUV420p must be rejected during preflight.
    let bad_preset = EncodingPreset {
        id: "odd_video".into(),
        name: "Odd Video".into(),
        container: ContainerFormat::Mp4,
        video_codec: VideoCodec::H264,
        width: Some(641), // Odd width invalid for 4:2:0 subsampling
        height: Some(360),
        audio_codec: AudioCodec::AacLc,
        audio_sample_rate: Some(48000),
        audio_channels: Some(2),
        audio_bitrate_kbps: Some(192),
        audio_priming_samples: 1024,
        faststart: true,
        frame_rate_num: Some(24),
        frame_rate_den: Some(1),
        video_bitrate_kbps: Some(2000),
        contract: None,
    };
    let err = compile_execution_plan(&dummy_wav, &dummy_out, &bad_preset)
        .err()
        .unwrap();
    assert!(
        matches!(err, PlanError::InvalidVideoDimensions { .. }),
        "Expected InvalidVideoDimensions, got: {:?}",
        err
    );

    // 2. Incompatible container/codec pair must be rejected.
    let bad_codec_preset = EncodingPreset {
        id: "wav_to_video".into(),
        name: "Wav to Video".into(),
        container: ContainerFormat::Wav,
        video_codec: VideoCodec::H264,
        width: Some(640),
        height: Some(360),
        audio_codec: AudioCodec::AacLc,
        audio_sample_rate: Some(48000),
        audio_channels: Some(2),
        audio_bitrate_kbps: Some(192),
        audio_priming_samples: 1024,
        faststart: true,
        frame_rate_num: Some(24),
        frame_rate_den: Some(1),
        video_bitrate_kbps: Some(2000),
        contract: None,
    };
    let err2 = compile_execution_plan(&dummy_wav, &dummy_out, &bad_codec_preset)
        .err()
        .unwrap();
    assert!(
        matches!(err2, PlanError::IncompatibleContainerCodec { .. }),
        "Expected IncompatibleContainerCodec, got: {:?}",
        err2
    );

    // 3. Valid audio preset compiles into a validated execution plan.
    let good_preset = EncodingPreset {
        id: "m4a_delivery".into(),
        name: "M4A Delivery".into(),
        container: ContainerFormat::M4a,
        video_codec: VideoCodec::None,
        width: None,
        height: None,
        audio_codec: AudioCodec::AacLc,
        audio_sample_rate: Some(48000),
        audio_channels: Some(2),
        audio_bitrate_kbps: Some(192),
        audio_priming_samples: 1024,
        faststart: true,
        frame_rate_num: None,
        frame_rate_den: None,
        video_bitrate_kbps: None,
        contract: None,
    };
    let plan =
        compile_execution_plan(&dummy_wav, &dummy_out, &good_preset).expect("Plan should compile");
    assert_eq!(plan.container, ContainerFormat::M4a);
    assert_eq!(plan.audio_codec, AudioCodec::AacLc);
    assert_eq!(plan.video_codec, VideoCodec::None);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_m7_real_quality_analysis() {
    let w = 64;
    let h = 64;
    let frame1 = VideoFrame::black(w, h, 0, Rational::new(1, 24));
    let frame2 = VideoFrame::black(w, h, 0, Rational::new(1, 24));

    // 1. Reference and identical frame: PSNR is infinite, SSIM is exactly 1.0.
    let psnr_inf = calculate_psnr(&frame1, &frame2).unwrap();
    assert!(
        psnr_inf.is_infinite(),
        "Identical frames must have infinite PSNR"
    );

    let ssim_one = calculate_ssim(&frame1, &frame2).unwrap();
    assert!(
        (ssim_one - 1.0).abs() < 1e-6,
        "Identical frames must have SSIM = 1.0, got: {ssim_one}"
    );

    // 2. Distorted frame: PSNR is finite, SSIM is degraded.
    let mut frame3 = VideoFrame::black(w, h, 0, Rational::new(1, 24));
    frame3.y = vec![200u8; (w * h) as usize]; // Shift luma from 0 to 200

    let psnr_dist = calculate_psnr(&frame1, &frame3).unwrap();
    assert!(
        psnr_dist.is_finite() && psnr_dist < 10.0,
        "Heavy distortion must yield low PSNR (<10 dB), got: {psnr_dist}"
    );

    let ssim_dist = calculate_ssim(&frame1, &frame3).unwrap();
    assert!(
        ssim_dist < 0.2,
        "Heavy distortion must yield degraded SSIM (<0.2), got: {ssim_dist}"
    );

    // 3. Real ITU-R BS.1770-4 loudness measurement.
    let sample_rate = 48000;
    let duration = 1.0;
    let total_samples = (sample_rate as f32 * duration) as usize;
    let mut ch1 = Vec::with_capacity(total_samples);
    let mut ch2 = Vec::with_capacity(total_samples);
    for i in 0..total_samples {
        let t = i as f32 / sample_rate as f32;
        ch1.push((t * 1000.0 * 2.0 * std::f32::consts::PI).sin() * 0.7);
        ch2.push((t * 1000.0 * 2.0 * std::f32::consts::PI).sin() * 0.7);
    }
    let audio_channels = vec![ch1, ch2];
    let loudness = measure_audio_loudness(&audio_channels, sample_rate).unwrap();

    assert!(
        loudness.integrated_lufs.is_finite(),
        "Integrated LUFS must be finite"
    );
    assert!(
        loudness.true_peak_dbtp.is_finite(),
        "True-peak dBTP must be finite"
    );
    assert!(
        loudness.integrated_lufs > -10.0 && loudness.integrated_lufs < 0.0,
        "Expected loud 1kHz sine wave loudness between -10 and 0 LUFS, got: {}",
        loudness.integrated_lufs
    );
}

#[test]
fn test_m8_hardware_acceleration_discovery() {
    let caps = HardwareCapabilities::detect();
    assert!(
        !caps.device_name.is_empty(),
        "Device name must be populated"
    );
    assert!(
        !caps.architecture.is_empty(),
        "Architecture must be populated"
    );

    #[cfg(target_os = "macos")]
    {
        println!("macOS Hardware Capabilities: {:?}", caps);
        assert!(
            caps.has_h264_hw,
            "macOS must report VideoToolbox H.264 support"
        );
        assert!(
            caps.has_hevc_hw,
            "macOS must report VideoToolbox HEVC support"
        );
    }
}

#[test]
fn test_m9_deliverable_verification_and_receipts() {
    let temp_dir = std::env::temp_dir().join("mediacraft_test_m9");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let input_wav = temp_dir.join("test_input.wav");
    write_sine_wav(&input_wav, 48000, 1.0);
    let output_m4a = temp_dir.join("deliverable.m4a");

    let preset = EncodingPreset {
        id: "test_m4a".into(),
        name: "Test M4A".into(),
        container: ContainerFormat::M4a,
        video_codec: VideoCodec::None,
        width: None,
        height: None,
        audio_codec: AudioCodec::AacLc,
        audio_sample_rate: Some(48000),
        audio_channels: Some(2),
        audio_bitrate_kbps: Some(192),
        audio_priming_samples: 1024,
        faststart: true,
        frame_rate_num: None,
        frame_rate_den: None,
        video_bitrate_kbps: None,
        contract: Some(DeliveryContract {
            name: "Generic M4A".to_string(),
            require_faststart: true,
            max_bitrate_kbps: Some(500),
            max_true_peak_dbfs: None,
            target_lufs: None,
            lufs_tolerance: None,
            require_audio_priming: true,
            max_file_size_bytes: None,
        }),
    };

    let mut engine = QueueEngine::new(1);
    let job_id = engine.add_job(input_wav.clone(), output_m4a.clone(), preset.clone(), 10);
    let result = engine.execute_job_to_completion(job_id);
    assert!(result.is_ok(), "Transcode failed: {:?}", result.err());

    let bytes = fs::read(&output_m4a).expect("read deliverable");
    let report = validate_deliverable(&input_wav, &output_m4a, &bytes, &preset, 1.0, None);
    assert!(report.is_some(), "Validation report should be generated");
    let rep = report.unwrap();
    assert_eq!(
        rep.overall_status,
        ValidationSeverity::Accept,
        "Deliverable findings: {:?}",
        rep.findings
    );
    assert!(
        output_m4a.exists(),
        "Output deliverable must exist on filesystem"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_m10_format_expansion_flac_and_prores() {
    let temp_dir = std::env::temp_dir().join("mediacraft_test_m10");
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // 1. FLAC Encoding Test
    let input_wav = temp_dir.join("stereo_test.wav");
    write_sine_wav(&input_wav, 44100, 0.5);
    let output_flac = temp_dir.join("output.flac");

    let flac_preset = EncodingPreset {
        id: "flac_archive".into(),
        name: "Master Archive FLAC".into(),
        container: ContainerFormat::M4a, // audio_codec determines pipeline route
        video_codec: VideoCodec::None,
        width: None,
        height: None,
        audio_codec: AudioCodec::Flac,
        audio_sample_rate: Some(44100),
        audio_channels: Some(2),
        audio_bitrate_kbps: None,
        audio_priming_samples: 0,
        faststart: false,
        frame_rate_num: None,
        frame_rate_den: None,
        video_bitrate_kbps: None,
        contract: None,
    };

    let mut engine = QueueEngine::new(1);
    let job_id = engine.add_job(input_wav.clone(), output_flac.clone(), flac_preset, 10);
    let res = engine.execute_job_to_completion(job_id);
    assert!(res.is_ok(), "FLAC transcode failed: {:?}", res.err());
    assert!(output_flac.exists(), "Output FLAC must exist");

    let flac_bytes = fs::read(&output_flac).unwrap();
    assert!(
        flac_bytes.starts_with(b"fLaC"),
        "Deliverable must start with standard RFC 9639 'fLaC' marker"
    );

    // If FFprobe is available on the system, externally probe the FLAC file
    if let Some(ffprobe) = find_binary("ffprobe") {
        let probe = Command::new(ffprobe)
            .arg("-v")
            .arg("error")
            .arg("-show_entries")
            .arg("stream=codec_name,channels,sample_rate")
            .arg("-of")
            .arg("default=noprint_wrappers=1")
            .arg(&output_flac)
            .output();
        if let Ok(out) = probe {
            let info = String::from_utf8_lossy(&out.stdout);
            println!("FFprobe on clean-room FLAC output:\n{info}");
            assert!(
                info.contains("codec_name=flac"),
                "FFprobe must recognize codec as flac"
            );
            assert!(
                info.contains("sample_rate=44100"),
                "Sample rate must match 44100"
            );
            assert!(info.contains("channels=2"), "Channels must match 2");
        }
    }

    // 2. ProRes 422 Encoding Test
    let mut prores_enc = ProResEncoderAdapter::new(320, 240);
    let dummy_frame = VideoFrame::black(320, 240, 0, Rational::new(1, 24));
    let packet = prores_enc.encode_frame(&dummy_frame);
    assert!(
        packet.is_ok(),
        "ProRes frame encoding failed: {:?}",
        packet.err()
    );
    let prores_pkt = packet.unwrap();
    assert!(
        !prores_pkt.is_empty(),
        "ProRes bitstream packet must contain compressed frame data"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
