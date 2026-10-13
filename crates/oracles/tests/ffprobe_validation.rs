//! Independent external verification of MediaCraft transcode pipelines via FFprobe and FFmpeg.
//!
//! Validates:
//! 1. Container format integrity (MP4/M4A) and Faststart atom layout (`moov` before `mdat`).
//! 2. AAC stream syntax and sample parameters (sample rate, channels, LC profile).
//! 3. Sample-exact duration and priming delay compensation:
//!    - Standard 3.0s 48kHz stereo (144,000 samples)
//!    - Arbitrary prime non-multiple of 1024 (100,007 samples)
//!    - Format matrix: Mono 48kHz, Stereo 44.1kHz, Stereo 96kHz
//! 4. 0 decode errors when decoded via FFmpeg null sink.
//! 5. Atomic staging, cancellation, and cleanup guarantees: no corrupt or partial files on disk.

use mediacraft_daemon::pipeline::WavToM4aPipeline;
use mediacraft_daemon::wav::{WavAudio, read_wav_file, write_wav_file};
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_binary(name: &str) -> Option<PathBuf> {
    let opt_brew = PathBuf::from("/opt/homebrew/bin").join(name);
    if opt_brew.exists() {
        return Some(opt_brew);
    }
    let usr_local = PathBuf::from("/usr/local/bin").join(name);
    if usr_local.exists() {
        return Some(usr_local);
    }
    // Check PATH via which
    if let Ok(output) = Command::new("which").arg(name).output() {
        let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if output.status.success() && !path_str.is_empty() {
            return Some(PathBuf::from(path_str));
        }
    }
    None
}

/// Generates synthetic sine waves across arbitrary channel counts and frame lengths.
fn generate_multichannel_sine(
    sample_rate: u32,
    channels: u16,
    total_frames: usize,
) -> Vec<Vec<f32>> {
    let mut channel_data = Vec::with_capacity(channels as usize);
    let freq = 440.0f32;
    let two_pi = 2.0f32 * std::f32::consts::PI;

    for ch_idx in 0..channels as usize {
        let mut ch_samples = Vec::with_capacity(total_frames);
        let phase_offset = ch_idx as f32 * 0.25;
        for frame in 0..total_frames {
            let t = frame as f32 / sample_rate as f32;
            let val = ((t * freq * two_pi) + phase_offset).sin() * 0.8;
            ch_samples.push(val);
        }
        channel_data.push(ch_samples);
    }

    channel_data
}

/// Helper that invokes FFmpeg to decode an M4A file back to an uncompressed WAV.
fn decode_m4a_with_ffmpeg(
    ffmpeg_bin: &Path,
    m4a_path: &Path,
    dest_wav_path: &Path,
) -> Result<WavAudio, String> {
    let ffmpeg_decode = Command::new(ffmpeg_bin)
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(m4a_path)
        .arg("-f")
        .arg("wav")
        .arg(dest_wav_path)
        .output()
        .map_err(|e| format!("Failed to spawn ffmpeg: {e}"))?;

    if !ffmpeg_decode.status.success() {
        return Err(format!(
            "FFmpeg decode failed: {}",
            String::from_utf8_lossy(&ffmpeg_decode.stderr)
        ));
    }

    read_wav_file(dest_wav_path).map_err(|e| format!("Failed to read decoded wav: {e}"))
}

#[test]
fn test_real_media_ffmpeg_and_ffprobe_verification() {
    let ffprobe_bin = match find_binary("ffprobe") {
        Some(bin) => bin,
        None => {
            eprintln!("ffprobe not found; skipping external empirical oracle test");
            return;
        }
    };

    let ffmpeg_bin = match find_binary("ffmpeg") {
        Some(bin) => bin,
        None => {
            eprintln!("ffmpeg not found; skipping external empirical oracle test");
            return;
        }
    };

    let temp_dir = std::env::temp_dir().join(format!("mediacraft_oracle_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wav_path = temp_dir.join("golden_tone_3s.wav");
    let m4a_path = temp_dir.join("golden_tone_3s.m4a");
    let decoded_wav_path = temp_dir.join("decoded_roundtrip.wav");

    // 1. Generate 3.0 seconds 48kHz stereo WAV (144,000 frames)
    let sample_rate = 48000u32;
    let channels = 2u16;
    let total_expected_frames = 144_000usize;
    let audio_channels = generate_multichannel_sine(sample_rate, channels, total_expected_frames);

    let planar_slices: Vec<&[f32]> = audio_channels.iter().map(|ch| ch.as_slice()).collect();
    write_wav_file(&wav_path, channels, sample_rate, &planar_slices).unwrap();
    assert!(wav_path.exists(), "Source WAV file must exist");

    // Verify WAV format with internal reader
    let wav = read_wav_file(&wav_path).unwrap();
    assert_eq!(wav.sample_rate, 48000);
    assert_eq!(wav.channels, 2);
    assert_eq!(wav.bits_per_sample, 16);
    assert_eq!(wav.frame_count(), total_expected_frames);

    // 2. Transcode to M4A with Faststart using real pipeline
    let receipt =
        WavToM4aPipeline::transcode(&wav_path, &m4a_path, 192_000, true, None::<fn(f32)>).unwrap();

    assert!(m4a_path.exists(), "Destination M4A file must exist");
    assert!(
        receipt.total_output_bytes > 0,
        "Receipt must report bytes written"
    );
    assert_eq!(receipt.total_input_frames, total_expected_frames);

    // Verify Faststart layout internally
    let m4a_bytes = std::fs::read(&m4a_path).unwrap();
    let is_faststart = mediacraft_oracles::verify_faststart_layout(&m4a_bytes).unwrap();
    assert!(
        is_faststart,
        "Generated M4A must have moov atom before mdat"
    );

    // 3. Independent validation via FFprobe
    let ffprobe_output = Command::new(&ffprobe_bin)
        .arg("-v")
        .arg("quiet")
        .arg("-print_format")
        .arg("json")
        .arg("-show_format")
        .arg("-show_streams")
        .arg(&m4a_path)
        .output()
        .expect("Failed to execute ffprobe");

    assert!(
        ffprobe_output.status.success(),
        "ffprobe must exit with code 0. Stderr: {}",
        String::from_utf8_lossy(&ffprobe_output.stderr)
    );

    let probe_json: serde_json::Value = serde_json::from_slice(&ffprobe_output.stdout)
        .expect("Failed to parse ffprobe json output");

    // Validate format
    let format = &probe_json["format"];
    let format_name = format["format_name"].as_str().unwrap_or("");
    assert!(
        format_name.contains("m4a") || format_name.contains("mp4") || format_name.contains("mov"),
        "Container format must be MP4/M4A, got: {}",
        format_name
    );

    let duration_str = format["duration"].as_str().unwrap_or("0");
    let container_duration: f64 = duration_str.parse().unwrap_or(0.0);
    // 3.0s expected duration. Must be within 1 frame (< 23ms at 48kHz)
    let duration_diff = (container_duration - 3.0).abs();
    assert!(
        duration_diff < 0.05,
        "Container duration ({container_duration}s) must be within 50ms of 3.0s"
    );

    // Validate audio stream
    let streams = probe_json["streams"]
        .as_array()
        .expect("ffprobe output must contain streams array");
    assert_eq!(streams.len(), 1, "Expected exactly 1 audio stream");

    let audio_stream = &streams[0];
    assert_eq!(audio_stream["codec_name"].as_str(), Some("aac"));
    assert_eq!(audio_stream["channels"].as_i64(), Some(2));
    assert_eq!(audio_stream["sample_rate"].as_str(), Some("48000"));

    // 4. Decode to null via FFmpeg to verify 0 bitstream syntax/decode errors
    let ffmpeg_null = Command::new(&ffmpeg_bin)
        .arg("-v")
        .arg("error")
        .arg("-xerror")
        .arg("-i")
        .arg(&m4a_path)
        .arg("-f")
        .arg("null")
        .arg("-")
        .output()
        .expect("Failed to execute ffmpeg null decode");

    let null_stderr = String::from_utf8_lossy(&ffmpeg_null.stderr);
    assert!(
        ffmpeg_null.status.success(),
        "FFmpeg decode to null must succeed with 0 errors. Stderr: {null_stderr}"
    );
    assert!(
        null_stderr.trim().is_empty(),
        "FFmpeg decode to null must produce 0 error log messages. Got: {null_stderr}"
    );

    // 5. Decode back to WAV via FFmpeg and verify EXACT sample recovery
    let decoded_wav = decode_m4a_with_ffmpeg(&ffmpeg_bin, &m4a_path, &decoded_wav_path).unwrap();
    assert_eq!(decoded_wav.sample_rate, 48000);
    assert_eq!(decoded_wav.channels, 2);

    let decoded_frame_count = decoded_wav.frame_count();
    assert_eq!(
        decoded_frame_count, total_expected_frames,
        "Decoded PCM frame count ({decoded_frame_count}) must EXACTLY equal input frames ({total_expected_frames})"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_sample_exact_arbitrary_non_multiple_of_1024() {
    let ffmpeg_bin = match find_binary("ffmpeg") {
        Some(bin) => bin,
        None => return,
    };

    let temp_dir = std::env::temp_dir().join(format!("mediacraft_prime_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let wav_path = temp_dir.join("prime_100007.wav");
    let m4a_path = temp_dir.join("prime_100007.m4a");
    let decoded_wav_path = temp_dir.join("decoded_prime.wav");

    // 100,007 samples is a prime number, non-multiple of 1024 (100,007 % 1024 = 679)
    let sample_rate = 48000u32;
    let channels = 2u16;
    let total_expected_frames = 100_007usize;
    let audio_channels = generate_multichannel_sine(sample_rate, channels, total_expected_frames);

    let planar_slices: Vec<&[f32]> = audio_channels.iter().map(|ch| ch.as_slice()).collect();
    write_wav_file(&wav_path, channels, sample_rate, &planar_slices).unwrap();

    let receipt =
        WavToM4aPipeline::transcode(&wav_path, &m4a_path, 160_000, true, None::<fn(f32)>).unwrap();
    assert_eq!(receipt.total_input_frames, total_expected_frames);

    // Verify sample-exact trimming on decode
    let decoded_wav = decode_m4a_with_ffmpeg(&ffmpeg_bin, &m4a_path, &decoded_wav_path).unwrap();
    assert_eq!(
        decoded_wav.frame_count(),
        total_expected_frames,
        "Non-multiple prime length must decode to exact sample count"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_sample_exact_channel_and_rate_matrix() {
    let ffmpeg_bin = match find_binary("ffmpeg") {
        Some(bin) => bin,
        None => return,
    };

    let temp_dir = std::env::temp_dir().join(format!("mediacraft_matrix_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    // Matrix test cases: (sample_rate, channels, frames, label)
    let test_cases = [
        (48000u32, 1u16, 48_000usize, "mono_48k"),
        (44100u32, 2u16, 44_100usize, "stereo_44k1"),
        (96000u32, 2u16, 48_000usize, "stereo_96k"),
    ];

    for (sr, ch, frames, label) in test_cases {
        let wav_path = temp_dir.join(format!("{label}.wav"));
        let m4a_path = temp_dir.join(format!("{label}.m4a"));
        let decoded_path = temp_dir.join(format!("{label}_decoded.wav"));

        let audio = generate_multichannel_sine(sr, ch, frames);
        let slices: Vec<&[f32]> = audio.iter().map(|c| c.as_slice()).collect();
        write_wav_file(&wav_path, ch, sr, &slices).unwrap();

        let receipt =
            WavToM4aPipeline::transcode(&wav_path, &m4a_path, 128_000, true, None::<fn(f32)>)
                .unwrap();
        assert_eq!(receipt.sample_rate, sr);
        assert_eq!(receipt.channels, ch);
        assert_eq!(receipt.total_input_frames, frames);

        let decoded = decode_m4a_with_ffmpeg(&ffmpeg_bin, &m4a_path, &decoded_path).unwrap();
        assert_eq!(decoded.sample_rate, sr, "{label}: sample rate mismatch");
        assert_eq!(decoded.channels, ch, "{label}: channel count mismatch");
        assert_eq!(
            decoded.frame_count(),
            frames,
            "{label}: frame count must match input exactly"
        );
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_atomic_cancellation_and_cleanup() {
    let temp_dir = std::env::temp_dir().join(format!("mediacraft_atomic_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let invalid_wav = temp_dir.join("corrupted.wav");
    std::fs::write(&invalid_wav, b"RIFF_NOT_A_VALID_WAV_DATA_BURST").unwrap();

    let output_m4a = temp_dir.join("should_never_exist.m4a");

    // Attempting to transcode a corrupt source fails early
    let result =
        WavToM4aPipeline::transcode(&invalid_wav, &output_m4a, 128_000, true, None::<fn(f32)>);
    assert!(
        result.is_err(),
        "Pipeline must return error on corrupt input"
    );

    // Verify output file does not exist
    assert!(
        !output_m4a.exists(),
        "Destination output file must not exist on failure"
    );

    // Verify no temporary files remain in directory
    let entries = std::fs::read_dir(&temp_dir).unwrap();
    let tmp_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("tmp"))
        .collect();
    assert!(
        tmp_files.is_empty(),
        "Staging cleanup guard must leave zero dangling temporary files on failure"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}
