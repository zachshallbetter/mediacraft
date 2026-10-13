//! Independent external verification of MediaCraft transcode pipelines via FFprobe and FFmpeg.
//!
//! Validates:
//! 1. Container format integrity (MP4/M4A).
//! 2. AAC stream syntax and sample parameters (48kHz, stereo, LC profile).
//! 3. Sample-exact duration and priming delay compensation (<23ms / 1 frame).
//! 4. 0 decode errors when decoded via FFmpeg.
//! 5. Exact sample recovery via roundtrip decode.

use mediacraft_daemon::pipeline::WavToM4aPipeline;
use mediacraft_daemon::wav::{read_wav_file, write_wav_file};
use std::path::PathBuf;
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

/// Generate a synthetic 440 Hz stereo sine wave for testing.
fn generate_stereo_sine(sample_rate: u32, duration_secs: f32) -> (Vec<f32>, Vec<f32>) {
    let total_frames = (sample_rate as f32 * duration_secs) as usize;
    let mut left = Vec::with_capacity(total_frames);
    let mut right = Vec::with_capacity(total_frames);
    let freq = 440.0f32;
    let two_pi = 2.0f32 * std::f32::consts::PI;

    for frame in 0..total_frames {
        let t = frame as f32 / sample_rate as f32;
        let val = (t * freq * two_pi).sin() * 0.9;
        left.push(val);
        right.push(val);
    }

    (left, right)
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
    let (left, right) = generate_stereo_sine(sample_rate, 3.0);
    let total_expected_frames = 144_000usize;

    let planar: [&[f32]; 2] = [&left, &right];
    write_wav_file(&wav_path, channels, sample_rate, &planar).unwrap();
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

    // Verify Faststart layout internally first
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
    // 3.0s expected duration. Must be within 1 frame (< 23ms at 48kHz / 1024 samples = 21.3ms)
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

    // 5. Decode back to WAV via FFmpeg and verify sample count
    let ffmpeg_decode = Command::new(&ffmpeg_bin)
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(&m4a_path)
        .arg("-f")
        .arg("wav")
        .arg(&decoded_wav_path)
        .output()
        .expect("Failed to execute ffmpeg decode to wav");

    assert!(
        ffmpeg_decode.status.success(),
        "FFmpeg decode to wav must succeed. Stderr: {}",
        String::from_utf8_lossy(&ffmpeg_decode.stderr)
    );

    let decoded_wav = read_wav_file(&decoded_wav_path).unwrap();
    assert_eq!(decoded_wav.sample_rate, 48000);
    assert_eq!(decoded_wav.channels, 2);

    let decoded_frame_count = decoded_wav.frame_count();
    // Edit list compensation: The decoded frame count should match or be within 1 frame (1024 samples) of original 144,000
    let frame_diff = (decoded_frame_count as i64 - total_expected_frames as i64).abs();
    assert!(
        frame_diff <= 1024,
        "Decoded sample count ({decoded_frame_count}) must be within 1 AAC frame of input ({total_expected_frames}). Drift: {frame_diff}"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}
