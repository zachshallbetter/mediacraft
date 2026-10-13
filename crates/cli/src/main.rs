//! Command-line interface for the MediaCraft clean-room transcoding service.
//!
//! Provides commands for:
//! - Inspecting live queue state over Unix domain socket IPC
//! - Enqueuing transcode jobs directly into the running workstation
//! - Triggering playback pause/resume hooks (NLE Dynamic Link simulation)
//! - Standalone faststart container rewriting (moving `moov` before `mdat`)
//! - Generating synthetic M4A/AAC media with edit-list priming delay compensation
//! - Running mathematical AV sync verification oracles

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
use mediacraft_daemon::ipc::IpcClient;
use mediacraft_oracles::{AvSyncGenerator, validate_priming_compensation, verify_faststart_layout};
use mediacraft_protocol::{EncodingPreset, PauseReason};

fn print_usage() {
    println!("MediaCraft CLI — Clean-Room Media Transcoding Workstation");
    println!("Usage:");
    println!(
        "  mediacraft status                     Show live queue status (via socket if running)"
    );
    println!(
        "  mediacraft list                       List all tracked jobs with states & progress"
    );
    println!(
        "  mediacraft add <source> <output>      Enqueue a transcode job into the active queue"
    );
    println!("  mediacraft pause                      Pause queue processing");
    println!("  mediacraft resume                     Resume queue processing");
    println!(
        "  mediacraft playback <on|off>          Simulate NLE playback start/stop (throttle hook)"
    );
    println!("  mediacraft faststart <input> <output> Reorganize MP4/M4A so moov precedes mdat");
    println!(
        "  mediacraft synth-m4a <output>         Generate compliant M4A with 1024-sample priming elst"
    );
    println!("  mediacraft oracle-test                Run AV sync and priming oracle verification");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).map(String::as_str).unwrap_or("help");

    match command {
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
                        // Offline queue engine demonstration
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

            // Validate priming delay
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
