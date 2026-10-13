//! Command-line interface for the MediaCraft clean-room transcoding service.
//!
//! Provides comprehensive CLI operations (Milestone 9):
//! - `transcode`: Offline or live execution of contract-validated transcode plans
//! - `probe`: Deep structural inspection of media containers and streams
//! - `verify`: Delivery contract compliance verification and preflight reporting
//! - `caps`: Runtime hardware acceleration discovery and codec support matrix
//! - `status` / `list` / `add` / `pause` / `resume` / `playback`: IPC workstation control
//! - `faststart` / `synth-m4a` / `oracle-test`: Low-level container and oracle utilities

#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unimplemented,
        clippy::todo,
        clippy::unreachable
    )
)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use std::env;
use std::fs;
use std::path::PathBuf;

use mediacraft_bitstream::isobmff::{build_synthetic_m4a, reorganize_to_faststart};
use mediacraft_daemon::QueueEngine;
use mediacraft_daemon::hardware::HardwareCapabilities;
use mediacraft_daemon::ipc::IpcClient;
use mediacraft_daemon::pipeline::video_demux_adapter::VideoDemuxer;
use mediacraft_daemon::validate_deliverable;
use mediacraft_daemon::wav::read_wav_file;
use mediacraft_oracles::{AvSyncGenerator, validate_priming_compensation, verify_faststart_layout};
use mediacraft_protocol::{ContainerFormat, EncodingPreset, PauseReason, ValidationSeverity};

fn print_usage() {
    println!("MediaCraft CLI — Clean-Room Media Transcoding Workstation");
    println!("Usage:");
    println!("  mediacraft transcode <source> <output> [--crf <n>] [--bitrate <kbps>]");
    println!("                                         Execute plan transcode headlessly");
    println!(
        "  mediacraft probe <input>               Inspect container tracks, resolution, and audio"
    );
    println!(
        "  mediacraft verify <deliverable>        Validate deliverable against broadcast contract"
    );
    println!(
        "  mediacraft caps                        Show host hardware acceleration & codec support"
    );
    println!("  mediacraft status                      Show live queue status via IPC");
    println!(
        "  mediacraft list                        List all tracked jobs with states & progress"
    );
    println!(
        "  mediacraft add <source> <output>       Enqueue transcode job into active workstation"
    );
    println!("  mediacraft pause                       Pause queue processing");
    println!("  mediacraft resume                      Resume queue processing");
    println!(
        "  mediacraft playback <on|off>           Simulate NLE playback start/stop (throttle hook)"
    );
    println!("  mediacraft faststart <input> <output>  Reorganize MP4/M4A so moov precedes mdat");
    println!(
        "  mediacraft synth-m4a <output>          Generate compliant M4A with 1024 priming elst"
    );
    println!(
        "  mediacraft oracle-test                 Run AV sync and priming oracle verification"
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("help");

    match command {
        "transcode" => {
            let source = args.get(2).map(PathBuf::from);
            let output = args.get(3).map(PathBuf::from);

            let (Some(src), Some(out)) = (source, output) else {
                eprintln!(
                    "Usage: mediacraft transcode <source> <output> [--crf <n>] [--bitrate <kbps>]"
                );
                return;
            };

            if !src.exists() {
                eprintln!("Error: Source file does not exist: {}", src.display());
                return;
            }

            let mut preset = EncodingPreset::default();
            // Parse container extension
            let out_ext = out
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            match out_ext.as_str() {
                "m4a" => {
                    preset.container = ContainerFormat::M4a;
                    preset.video_codec = mediacraft_protocol::VideoCodec::None;
                    preset.audio_codec = mediacraft_protocol::AudioCodec::AacLc;
                }
                "wav" => {
                    preset.container = ContainerFormat::Wav;
                    preset.video_codec = mediacraft_protocol::VideoCodec::None;
                    preset.audio_codec = mediacraft_protocol::AudioCodec::Pcm16Le;
                }
                "mov" => {
                    preset.container = ContainerFormat::Mov;
                }
                _ => {
                    preset.container = ContainerFormat::Mp4;
                }
            }

            // Parse optional flags
            let mut i = 4;
            while i < args.len() {
                if args.get(i).map(|s| s.as_str()) == Some("--bitrate") {
                    if let Some(b) = args
                        .get(i.saturating_add(1))
                        .and_then(|val| val.parse::<u32>().ok())
                    {
                        preset.video_bitrate_kbps = Some(b);
                    }
                    i = i.saturating_add(2);
                    continue;
                }
                i = i.saturating_add(1);
            }

            println!("Executing Headless Transcode Plan:");
            println!("  Source:    {}", src.display());
            println!("  Output:    {}", out.display());
            println!("  Container: {:?}", preset.container);
            println!("  Video:     {:?}", preset.video_codec);
            println!("  Audio:     {:?}", preset.audio_codec);

            let mut engine = QueueEngine::new(1);
            let job_id = engine.add_job(src.clone(), out.clone(), preset, 10);
            match engine.execute_job_to_completion(job_id) {
                Ok((duration, bytes)) => {
                    println!("\n✓ Transcode Completed Successfully!");
                    println!("  Duration:      {duration:.2} s");
                    println!("  Deliverable:   {bytes} bytes ({})", out.display());

                    let receipt_file = out.with_extension(format!(
                        "{}.receipt.json",
                        out.extension().and_then(|s| s.to_str()).unwrap_or("out")
                    ));
                    if receipt_file.exists() {
                        println!("  Receipt:       {}", receipt_file.display());
                    }

                    let contract_file = out.with_extension(format!(
                        "{}.contract.json",
                        out.extension().and_then(|s| s.to_str()).unwrap_or("out")
                    ));
                    if contract_file.exists() {
                        println!("  Validation:    {}", contract_file.display());
                    }
                }
                Err(e) => {
                    eprintln!("\n✗ Transcode Failed: {e}");
                }
            }
        }
        "probe" => {
            let input = args.get(2).map(PathBuf::from);
            let Some(path) = input else {
                eprintln!("Usage: mediacraft probe <input_file>");
                return;
            };

            if !path.exists() {
                eprintln!("Error: File does not exist: {}", path.display());
                return;
            }

            println!("Media Probe: {}", path.display());

            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();

            if ext == "wav" {
                match read_wav_file(&path) {
                    Ok(wav) => {
                        let dur = if wav.sample_rate > 0 {
                            wav.samples.first().map(|c| c.len()).unwrap_or(0) as f64
                                / wav.sample_rate as f64
                        } else {
                            0.0
                        };
                        println!("  Format:          RIFF / Linear PCM WAVE");
                        println!("  Sample Rate:     {} Hz", wav.sample_rate);
                        println!("  Channels:        {}", wav.channels);
                        println!("  Bits per sample: {} bit", wav.bits_per_sample);
                        println!("  Duration:        {dur:.3} s");
                    }
                    Err(e) => eprintln!("Failed to parse WAV file: {e}"),
                }
            } else {
                match VideoDemuxer::inspect(&path) {
                    Ok(info) => {
                        println!("  Format:          ISOBMFF ({})", ext.to_uppercase());
                        println!("  Video Track:     {}x{}", info.width, info.height);
                        println!("  Timescale:       {} Hz", info.timescale);
                        println!("  Total Samples:   {}", info.sample_count);
                        println!("  Duration:        {:.3} s", info.duration_seconds);
                    }
                    Err(_) => {
                        println!("  Format:          Binary Media Container ({})", ext);
                        if let Ok(meta) = fs::metadata(&path) {
                            println!("  File Size:       {} bytes", meta.len());
                        }
                    }
                }
            }
        }
        "verify" => {
            let target = args.get(2).map(PathBuf::from);
            let Some(out_p) = target else {
                eprintln!("Usage: mediacraft verify <deliverable_file>");
                return;
            };

            if !out_p.exists() {
                eprintln!("Error: File does not exist: {}", out_p.display());
                return;
            }

            let bytes = match fs::read(&out_p) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Failed to read deliverable: {e}");
                    return;
                }
            };

            let preset = EncodingPreset::default();
            let duration = 1.0; // Standard evaluation duration
            let src = out_p.clone();

            if let Some(report) =
                validate_deliverable(&src, &out_p, &bytes, &preset, duration, None)
            {
                println!("Delivery Contract Verification Report:");
                println!("  Target:   {}", report.target_file.display());
                println!("  Contract: {}", report.contract_name);
                println!("  Status:   {:?}\n", report.overall_status);

                for f in &report.findings {
                    let mark = if f.passed { "✓ PASS" } else { "✗ FAIL" };
                    println!(
                        "  [{mark}] {:<35} | Observed: {:<20} | Expected: {}",
                        f.criterion, f.observed, f.expected
                    );
                }

                if report.overall_status == ValidationSeverity::Accept {
                    println!("\nVerdict: DELIVERABLE ACCEPTED");
                } else {
                    println!("\nVerdict: DELIVERABLE REJECTED");
                }
            } else {
                println!("No contract attached to preset.");
            }
        }
        "caps" => {
            let caps = HardwareCapabilities::detect();
            println!("MediaCraft Host Capabilities Discovery:");
            println!("  Device Name:            {}", caps.device_name);
            println!("  Architecture:           {}", caps.architecture);
            println!(
                "  Hardware H.264 Encoder: {}",
                if caps.has_h264_hw {
                    "YES (VideoToolbox)"
                } else {
                    "NO (Software fallback)"
                }
            );
            println!(
                "  Hardware HEVC Encoder:  {}",
                if caps.has_hevc_hw {
                    "YES (VideoToolbox)"
                } else {
                    "NO (Software fallback)"
                }
            );
            println!(
                "  Hardware ProRes Engine: {}",
                if caps.has_prores_hw {
                    "YES (Apple Silicon)"
                } else {
                    "NO (Clean-room fallback)"
                }
            );
            println!(
                "  Preferred Backend:      {}",
                caps.preferred_backend.description()
            );
            println!("\nClean-Room Codec Matrix:");
            println!(
                "  • Video: H.264 (filmcraft-h264enc), ProRes (filmcraft-prores), HEVC (filmcraft-hevc)"
            );
            println!(
                "  • Audio: AAC-LC (filmcraft-aac), FLAC (filmcraft-flac), Opus (filmcraft-opus), PCM (wav)"
            );
            println!(
                "  • Multiplexers: MP4/M4A/MOV (filmcraft-isobmff), Faststart (mediacraft-bitstream)"
            );
        }
        "status" => {
            if let Ok(mut client) = IpcClient::connect_default() {
                match client.get_status() {
                    Ok(queue) => {
                        println!("Live MediaCraft Workstation Queue Status (IPC Connected):");
                        println!("  Running:        {}", queue.is_running);
                        if let Some(reason) = queue.pause_reason {
                            println!("  Pause Reason:   {:?}", reason);
                        }
                        println!("  Active Jobs:    {}", queue.active_jobs);
                        println!("  Queued Jobs:    {}", queue.queued_jobs);
                        println!("  Completed Jobs: {}", queue.completed_jobs);
                        println!("  Failed Jobs:    {}", queue.failed_jobs);
                    }
                    Err(e) => eprintln!("Error querying IPC daemon: {e}"),
                }
            } else {
                println!("No running MediaCraft workstation detected on /tmp/mediacraft.sock.");
                println!("(Start the GUI or background daemon to enable cross-app IPC)");
            }
        }
        "list" => {
            if let Ok(mut client) = IpcClient::connect_default() {
                match client.list_jobs() {
                    Ok(jobs) => {
                        println!("Tracked Jobs ({} total):", jobs.len());
                        for j in jobs {
                            println!(
                                "  • Job #{:<3} [{:?}] -> Output: {}",
                                j.id.0,
                                j.state,
                                j.output_path.display()
                            );
                        }
                    }
                    Err(e) => eprintln!("Error querying job list: {e}"),
                }
            } else {
                eprintln!("MediaCraft workstation not running. Start the application first.");
            }
        }
        "add" => {
            let source = args.get(2).map(PathBuf::from);
            let output = args.get(3).map(PathBuf::from);

            match (source, output) {
                (Some(s), Some(o)) => {
                    if let Ok(mut client) = IpcClient::connect_default() {
                        match client.add_job(s.clone(), o.clone(), None, Some(5)) {
                            Ok(job_id) => {
                                println!(
                                    "✓ Job {:?} enqueued into live workstation: {} -> {}",
                                    job_id,
                                    s.display(),
                                    o.display()
                                );
                            }
                            Err(e) => eprintln!("IPC add_job failed: {e}"),
                        }
                    } else {
                        let mut engine = QueueEngine::new(2);
                        let j_id = engine.add_job(s, o, EncodingPreset::default(), 5);
                        println!("Enqueued in local offline queue: Job {:?}", j_id);
                    }
                }
                _ => {
                    eprintln!("Error: missing <source> or <output> argument");
                    print_usage();
                }
            }
        }
        "pause" => {
            if let Ok(mut client) = IpcClient::connect_default() {
                let _ = client.pause_queue(PauseReason::UserRequested);
                println!("Queue paused.");
            } else {
                eprintln!("MediaCraft workstation not running.");
            }
        }
        "resume" => {
            if let Ok(mut client) = IpcClient::connect_default() {
                let _ = client.resume_queue(PauseReason::UserRequested);
                println!("Queue resumed.");
            } else {
                eprintln!("MediaCraft workstation not running.");
            }
        }
        "playback" => {
            let state = args.get(2).map(String::as_str).unwrap_or("on");
            let is_playing = state == "on" || state == "true" || state == "start";
            if let Ok(mut client) = IpcClient::connect_default() {
                match client.set_playback_active(is_playing) {
                    Ok(status) => {
                        println!(
                            "Playback state set to {}. Queue is now: {}",
                            if is_playing { "PLAYING" } else { "STOPPED" },
                            if status.is_running {
                                "RUNNING"
                            } else {
                                "PAUSED"
                            }
                        );
                    }
                    Err(e) => eprintln!("IPC playback hook failed: {e}"),
                }
            } else {
                eprintln!("MediaCraft workstation not running.");
            }
        }
        "faststart" => {
            let in_path = args.get(2).map(PathBuf::from);
            let out_path = args.get(3).map(PathBuf::from);

            match (in_path, out_path) {
                (Some(in_p), Some(out_p)) => match fs::read(&in_p) {
                    Ok(data) => match reorganize_to_faststart(&data) {
                        Ok(reorganized) => match fs::write(&out_p, &reorganized) {
                            Ok(_) => {
                                let was_faststart = verify_faststart_layout(&data).unwrap_or(false);
                                let is_faststart =
                                    verify_faststart_layout(&reorganized).unwrap_or(false);
                                println!("Faststart reorganization completed:");
                                println!(
                                    "  Input:         {} (faststart: {was_faststart})",
                                    in_p.display()
                                );
                                println!(
                                    "  Output:        {} (faststart: {is_faststart})",
                                    out_p.display()
                                );
                                println!("  Bytes Written: {} B", reorganized.len());
                            }
                            Err(e) => eprintln!("Failed to write output file: {e}"),
                        },
                        Err(e) => eprintln!("Faststart reorganizer failed: {e}"),
                    },
                    Err(e) => eprintln!("Failed to read input file: {e}"),
                },
                _ => {
                    eprintln!("Usage: mediacraft faststart <input_file> <output_file>");
                }
            }
        }
        "synth-m4a" => {
            let out_path = args.get(2).map(PathBuf::from);
            if let Some(out_p) = out_path {
                let pcm_dummy = vec![0u8; 4096];
                let m4a_bytes = build_synthetic_m4a(&pcm_dummy, 48_000, 2, 1024, true);
                match fs::write(&out_p, &m4a_bytes) {
                    Ok(_) => {
                        let is_fs = verify_faststart_layout(&m4a_bytes).unwrap_or(false);
                        println!("Synthetic M4A container generated successfully:");
                        println!("  File:          {}", out_p.display());
                        println!("  Size:          {} bytes", m4a_bytes.len());
                        println!("  Sample Rate:   48,000 Hz");
                        println!("  Channels:      2 (Stereo)");
                        println!("  Priming elst:  1024 samples offset (Sample-Accurate)");
                        println!("  Faststart:     {} (moov before mdat)", is_fs);
                    }
                    Err(e) => eprintln!("Failed to write file: {e}"),
                }
            } else {
                eprintln!("Usage: mediacraft synth-m4a <output_file>");
            }
        }
        "oracle-test" => {
            println!("Running AV Sync & Priming Verification Oracle...");
            let generator = match AvSyncGenerator::new(24, 48_000, 1) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("Failed to initialize AV generator: {e}");
                    return;
                }
            };
            println!("  Timebase: 24 fps, 48 kHz, flash interval 1.0s (24 frames)");
            let pcm = generator.generate_pcm16_samples(48);
            println!(
                "  Generated {} audio samples for 48 frames (2.0s)",
                pcm.len()
            );

            let res = validate_priming_compensation(1024, Some(1024));
            println!("  AAC-LC Priming delay: {} samples", res.priming_samples);
            println!("  Edit list media_time: {:?}", res.edit_list_media_time);
            println!("  Measured sync drift:  {} samples", res.sync_drift_samples);
            println!("  Synchronized:         {}", res.is_synchronized);
            if res.is_synchronized {
                println!("PASS: Oracle verification succeeded with zero sample drift.");
            } else {
                eprintln!("FAIL: Drift detected in priming compensation!");
            }
        }
        _ => {
            print_usage();
        }
    }
}
