//! Dedicated Transcoding Queue Workspace (MC-01 to MC-05)
//!
//! Provides the primary encoding pipeline workspace, supporting both empty-state
//! first-launch ingest affordances (MC-01) and populated active batch transcode operations (MC-02..MC-05).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use egui::{Color32, CornerRadius, FontId, Pos2, Rect, RichText, Stroke, Vec2};
use mediacraft_daemon::QueueEngine;
use mediacraft_protocol::{EncodingPreset, JobId, JobState, PauseReason};

pub struct QueueViewState {
    pub selected_job_id: Option<JobId>,
    pub sweep_phase: f32,
}

impl Default for QueueViewState {
    fn default() -> Self {
        Self {
            selected_job_id: None,
            sweep_phase: 0.0,
        }
    }
}

pub fn render_queue_workspace(
    ui: &mut egui::Ui,
    state: &mut QueueViewState,
    engine: &Arc<Mutex<QueueEngine>>,
    presets: &[EncodingPreset],
    on_edit_preset: &mut Option<EncodingPreset>,
) {
    let mut eng = engine.lock().unwrap_or_else(PoisonError::into_inner);
    let jobs = eng.list_jobs();
    let status = eng.status();

    if jobs.is_empty() {
        render_empty_queue(ui, &mut eng, presets);
    } else {
        render_populated_queue(ui, state, &mut eng, presets, on_edit_preset, &status, &jobs);
    }
}

fn render_empty_queue(ui: &mut egui::Ui, engine: &mut QueueEngine, presets: &[EncodingPreset]) {
    ui.vertical_centered(|ui| {
        ui.add_space(40.0);

        // Dashed drop zone container
        let container_w = 720.0_f32.min(ui.available_width() - 40.0);
        let container_h = 420.0;
        let (rect, resp) = ui.allocate_exact_size(
            Vec2::new(container_w, container_h),
            egui::Sense::click(),
        );

        let painter = ui.painter_at(rect);
        painter.rect_filled(
            rect,
            CornerRadius::same(8),
            Color32::from_rgb(18, 22, 28),
        );
        painter.rect_stroke(
            rect,
            CornerRadius::same(8),
            Stroke::new(1.5, Color32::from_rgb(60, 75, 95)),
            egui::StrokeKind::Inside,
        );

        // Content inside dropzone
        let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(20.0)));
        child_ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(RichText::new("📥").size(42.0));
            ui.add_space(8.0);
            ui.heading(
                RichText::new("Drop Media Files or Project Timelines Here")
                    .size(19.0)
                    .color(Color32::from_rgb(240, 245, 255))
                    .strong(),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Supports camera raw, ProRes, DNxHR, H.264/HEVC, BWF audio, and image sequences.",
                )
                .size(11.5)
                .color(Color32::from_rgb(160, 175, 195)),
            );

            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 320.0) * 0.5);
                if ui
                    .button(
                        RichText::new("＋ Load Sample Transcode Queue")
                            .size(12.0)
                            .strong()
                            .color(Color32::from_rgb(90, 200, 255)),
                    )
                    .clicked()
                {
                    load_default_sample_jobs(engine, presets);
                }
                ui.add_space(8.0);
                if ui
                    .button(RichText::new("📁 Browse Files...").size(12.0))
                    .clicked()
                {
                    load_default_sample_jobs(engine, presets);
                }
            });

            ui.add_space(32.0);
            ui.separator();
            ui.add_space(12.0);

            ui.label(
                RichText::new("QUICK-START ENCODING PRESETS")
                    .size(10.5)
                    .color(Color32::from_rgb(130, 145, 165))
                    .strong(),
            );
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                let card_w = ((ui.available_width() - 36.0) / 4.0).max(120.0);
                let cards = [
                    (
                        "🎬 Web Delivery",
                        "H.264 1080p Full HD",
                        "8 Mbps • Faststart MP4",
                    ),
                    (
                        "❏ Editing Proxies",
                        "ProRes 422 Proxy",
                        "1080p • Low Compute",
                    ),
                    (
                        "🎞 Broadcast Master",
                        "ProRes 422 HQ",
                        "2160p • 24-bit Linear PCM",
                    ),
                    (
                        "🎵 Audio Master",
                        "AAC-LC 320 kbps",
                        "48 kHz Stereo Master",
                    ),
                ];

                for (title, sub, meta) in cards {
                    ui.allocate_ui(Vec2::new(card_w, 74.0), |ui| {
                        let (c_rect, _) = ui.allocate_exact_size(
                            Vec2::new(card_w, 74.0),
                            egui::Sense::hover(),
                        );
                        let p = ui.painter_at(c_rect);
                        p.rect_filled(
                            c_rect,
                            CornerRadius::same(5),
                            Color32::from_rgb(26, 32, 42),
                        );
                        p.rect_stroke(
                            c_rect,
                            CornerRadius::same(5),
                            Stroke::new(1.0, Color32::from_rgb(48, 58, 74)),
                            egui::StrokeKind::Inside,
                        );

                        p.text(
                            Pos2::new(c_rect.min.x + 8.0, c_rect.min.y + 10.0),
                            egui::Align2::LEFT_TOP,
                            title,
                            FontId::proportional(11.0),
                            Color32::from_rgb(230, 240, 255),
                        );
                        p.text(
                            Pos2::new(c_rect.min.x + 8.0, c_rect.min.y + 28.0),
                            egui::Align2::LEFT_TOP,
                            sub,
                            FontId::proportional(9.5),
                            Color32::from_rgb(140, 195, 255),
                        );
                        p.text(
                            Pos2::new(c_rect.min.x + 8.0, c_rect.min.y + 46.0),
                            egui::Align2::LEFT_TOP,
                            meta,
                            FontId::proportional(9.0),
                            Color32::GRAY,
                        );
                    });
                }
            });
        });

        if resp.clicked() {
            load_default_sample_jobs(engine, presets);
        }
    });
}

fn render_populated_queue(
    ui: &mut egui::Ui,
    state: &mut QueueViewState,
    engine: &mut QueueEngine,
    presets: &[EncodingPreset],
    on_edit_preset: &mut Option<EncodingPreset>,
    status: &mediacraft_protocol::QueueStatus,
    jobs: &[mediacraft_protocol::JobInfo],
) {
    // 1. Queue Action Toolbar
    ui.horizontal(|ui| {
        let is_running = status.is_running;
        let is_paused = status.pause_reason.is_some();

        if is_paused {
            if ui
                .button(
                    RichText::new("▶ Resume Queue")
                        .size(11.5)
                        .strong()
                        .color(Color32::from_rgb(90, 230, 130)),
                )
                .clicked()
            {
                engine.resume_queue(PauseReason::UserRequested);
            }
        } else if is_running && status.active_jobs > 0 {
            if ui
                .button(
                    RichText::new("⏸ Pause Queue")
                        .size(11.5)
                        .color(Color32::from_rgb(255, 190, 70)),
                )
                .clicked()
            {
                engine.pause_queue(PauseReason::UserRequested);
            }
        } else if ui
            .button(
                RichText::new("▶ Start Queue")
                    .size(11.5)
                    .strong()
                    .color(Color32::from_rgb(90, 230, 130)),
            )
            .clicked()
        {
            let _ = engine.poll_dispatch();
        }

        if ui
            .button(RichText::new("⏹ Stop").size(11.5).color(Color32::GRAY))
            .clicked()
        {
            engine.pause_queue(PauseReason::UserRequested);
        }

        ui.add_space(8.0);
        if ui
            .button(RichText::new("[-] Clear Done").size(11.5))
            .clicked()
        {
            engine.clear_completed();
        }

        if ui.button(RichText::new("🗑 Clear All").size(11.5)).clicked() {
            engine.clear_all();
        }

        ui.add_space(8.0);
        if ui
            .button(
                RichText::new("＋ Add Sample Job")
                    .size(11.5)
                    .color(Color32::from_rgb(90, 200, 255)),
            )
            .clicked()
        {
            load_default_sample_jobs(engine, presets);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (stat_label, stat_color) = if is_paused {
                ("PAUSED", Color32::from_rgb(255, 190, 70))
            } else if status.active_jobs > 0 {
                ("ENCODING", Color32::from_rgb(90, 230, 130))
            } else {
                ("IDLE", Color32::GRAY)
            };

            ui.label(
                RichText::new(format!(
                    "{} ({} Active • {} Queued • {} Done)",
                    stat_label, status.active_jobs, status.queued_jobs, status.completed_jobs
                ))
                .size(11.0)
                .strong()
                .color(stat_color),
            );
        });
    });

    ui.add_space(4.0);
    ui.separator();

    // 2. Queue Job Table Area
    let available_h = ui.available_height() - 200.0;
    egui::ScrollArea::vertical()
        .id_salt("populated_queue_table_scroll")
        .max_height(available_h.max(160.0))
        .show(ui, |ui| {
            egui::Grid::new("queue_jobs_grid")
                .striped(true)
                .num_columns(7)
                .spacing(Vec2::new(12.0, 8.0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Status")
                            .size(11.0)
                            .strong()
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("Source File")
                            .size(11.0)
                            .strong()
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("Preset / Codec")
                            .size(11.0)
                            .strong()
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("Destination")
                            .size(11.0)
                            .strong()
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("Progress")
                            .size(11.0)
                            .strong()
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("Speed / ETA")
                            .size(11.0)
                            .strong()
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("Actions")
                            .size(11.0)
                            .strong()
                            .color(Color32::GRAY),
                    );
                    ui.end_row();

                    let mut retry_id = None;
                    let mut cancel_id = None;
                    let mut remove_id = None;
                    let mut edit_preset_action = None;

                    for job in jobs {
                        // Status badge
                        let (state_str, state_color) = match job.state {
                            JobState::Queued => ("Queued", Color32::LIGHT_GRAY),
                            JobState::Running { .. } => {
                                ("Encoding", Color32::from_rgb(80, 230, 120))
                            }
                            JobState::Paused { .. } => ("Paused", Color32::from_rgb(255, 180, 50)),
                            JobState::Completed { .. } => ("Done", Color32::from_rgb(70, 160, 240)),
                            JobState::Failed { .. } => ("Failed", Color32::from_rgb(255, 70, 70)),
                            JobState::Cancelled => ("Cancelled", Color32::GRAY),
                        };

                        ui.label(
                            RichText::new(state_str)
                                .size(11.0)
                                .strong()
                                .color(state_color),
                        );

                        // Source File
                        let file_name = job
                            .source_path
                            .file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or("source");
                        let is_selected = state.selected_job_id == Some(job.id);
                        let file_label = if is_selected {
                            RichText::new(file_name)
                                .size(11.5)
                                .strong()
                                .color(Color32::from_rgb(90, 200, 255))
                        } else {
                            RichText::new(file_name).size(11.5).strong()
                        };
                        if ui.selectable_label(is_selected, file_label).clicked() {
                            state.selected_job_id = Some(job.id);
                        }

                        // Preset / Codec button
                        if ui
                            .link(
                                RichText::new(&job.preset.name)
                                    .size(11.0)
                                    .color(Color32::from_rgb(120, 190, 255)),
                            )
                            .clicked()
                        {
                            edit_preset_action = Some(job.preset.clone());
                        }

                        // Destination Path
                        let out_name = job
                            .output_path
                            .file_name()
                            .and_then(|f| f.to_str())
                            .unwrap_or("output");
                        ui.label(
                            RichText::new(out_name)
                                .size(10.5)
                                .color(Color32::from_rgb(170, 185, 205)),
                        );

                        // Progress
                        match job.state {
                            JobState::Running {
                                progress_pct,
                                current_frame,
                                total_frames,
                                ..
                            } => {
                                ui.horizontal(|ui| {
                                    ui.add(
                                        egui::ProgressBar::new(progress_pct)
                                            .desired_width(120.0)
                                            .text(format!("{:.0}%", progress_pct * 100.0)),
                                    );
                                    if total_frames > 0 {
                                        ui.label(
                                            RichText::new(format!(
                                                "{current_frame}/{total_frames}f"
                                            ))
                                            .size(9.5)
                                            .color(Color32::GRAY),
                                        );
                                    }
                                });
                            }
                            JobState::Paused { progress_pct, .. } => {
                                ui.add(
                                    egui::ProgressBar::new(progress_pct)
                                        .desired_width(120.0)
                                        .text(format!("{:.0}% (paused)", progress_pct * 100.0)),
                                );
                            }
                            JobState::Completed { duration_secs, .. } => {
                                ui.label(
                                    RichText::new(format!("100% ({:.1}s)", duration_secs))
                                        .size(10.5)
                                        .color(Color32::from_rgb(90, 230, 130)),
                                );
                            }
                            JobState::Failed { ref error_message } => {
                                ui.label(
                                    RichText::new(error_message)
                                        .size(10.0)
                                        .color(Color32::from_rgb(255, 80, 80)),
                                );
                            }
                            JobState::Queued => {
                                ui.label(
                                    RichText::new("Waiting...").size(10.5).color(Color32::GRAY),
                                );
                            }
                            JobState::Cancelled => {
                                ui.label(
                                    RichText::new("Cancelled").size(10.5).color(Color32::GRAY),
                                );
                            }
                        }

                        // Speed / ETA
                        if let JobState::Running {
                            encoding_fps,
                            progress_pct,
                            ..
                        } = job.state
                        {
                            let remaining_pct = (1.0 - progress_pct).max(0.0);
                            let eta_secs = if encoding_fps > 0.0 {
                                ((remaining_pct * 2400.0) / encoding_fps) as u32
                            } else {
                                14
                            };
                            ui.label(
                                RichText::new(format!(
                                    "{:.1} fps (ETA {}s)",
                                    encoding_fps, eta_secs
                                ))
                                .size(10.5)
                                .color(Color32::from_rgb(180, 215, 255)),
                            );
                        } else {
                            ui.label(RichText::new("—").size(10.5).color(Color32::DARK_GRAY));
                        }

                        // Actions
                        ui.horizontal(|ui| match job.state {
                            JobState::Running { .. } | JobState::Queued => {
                                if ui.small_button("✕").clicked() {
                                    cancel_id = Some(job.id);
                                }
                            }
                            JobState::Failed { .. } | JobState::Cancelled => {
                                if ui.small_button("⟳").clicked() {
                                    retry_id = Some(job.id);
                                }
                                if ui.small_button("🗑").clicked() {
                                    remove_id = Some(job.id);
                                }
                            }
                            JobState::Completed { .. } => {
                                if ui.small_button("🗑").clicked() {
                                    remove_id = Some(job.id);
                                }
                            }
                            JobState::Paused { .. } => {
                                if ui.small_button("▶").clicked() {
                                    engine.resume_queue(PauseReason::UserRequested);
                                }
                                if ui.small_button("✕").clicked() {
                                    cancel_id = Some(job.id);
                                }
                            }
                        });

                        ui.end_row();
                    }

                    if let Some(id) = cancel_id {
                        let _ = engine.cancel_job(id);
                    }
                    if let Some(id) = retry_id {
                        let _ = engine.retry_job(id);
                    }
                    if let Some(id) = remove_id {
                        let _ = engine.remove_job(id);
                    }
                    if let Some(p) = edit_preset_action {
                        *on_edit_preset = Some(p);
                    }
                });
        });

    ui.separator();

    // 3. Bottom Live Transcode Monitor (MC-03)
    render_transcode_monitor(ui, state, jobs);
}

fn render_transcode_monitor(
    ui: &mut egui::Ui,
    state: &mut QueueViewState,
    jobs: &[mediacraft_protocol::JobInfo],
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("LIVE TRANSCODE MONITOR")
                .size(10.5)
                .color(Color32::from_rgb(130, 145, 165))
                .strong(),
        );

        let active_job = jobs
            .iter()
            .find(|j| matches!(j.state, JobState::Running { .. }));
        if let Some(job) = active_job {
            let fname = job
                .source_path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("clip");
            ui.label(
                RichText::new(format!("● Encoding: {fname}"))
                    .size(10.5)
                    .color(Color32::from_rgb(90, 230, 130))
                    .strong(),
            );
        } else {
            ui.label(
                RichText::new("○ Queue Idle")
                    .size(10.5)
                    .color(Color32::GRAY),
            );
        }
    });

    ui.add_space(2.0);

    ui.horizontal(|ui| {
        // Left: 16:9 Video Raster with animated scanline sweep
        let (rect, _) = ui.allocate_exact_size(Vec2::new(260.0, 146.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);

        painter.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(12, 16, 22));
        painter.rect_stroke(
            rect,
            CornerRadius::same(4),
            Stroke::new(1.0, Color32::from_rgb(45, 55, 72)),
            egui::StrokeKind::Inside,
        );

        // Test raster pattern
        let bar_w = rect.width() / 7.0;
        let colors = [
            Color32::from_rgb(180, 180, 180),
            Color32::from_rgb(180, 180, 0),
            Color32::from_rgb(0, 180, 180),
            Color32::from_rgb(0, 180, 0),
            Color32::from_rgb(180, 0, 180),
            Color32::from_rgb(180, 0, 0),
            Color32::from_rgb(0, 0, 180),
        ];
        for (i, &col) in colors.iter().enumerate() {
            let bx = rect.min.x + (i as f32) * bar_w;
            painter.rect_filled(
                Rect::from_min_max(Pos2::new(bx, rect.min.y), Pos2::new(bx + bar_w, rect.max.y)),
                CornerRadius::ZERO,
                col,
            );
        }

        // Horizontal animated scanline sweep
        state.sweep_phase = (state.sweep_phase + 0.015).fract();
        let sweep_y = rect.min.y + rect.height() * state.sweep_phase;
        painter.line_segment(
            [
                Pos2::new(rect.min.x, sweep_y),
                Pos2::new(rect.max.x, sweep_y),
            ],
            Stroke::new(1.5, Color32::from_rgb(120, 240, 255)),
        );

        // Timecode & Frame overlay
        let tc_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + 6.0, rect.max.y - 22.0),
            Vec2::new(140.0, 16.0),
        );
        painter.rect_filled(
            tc_rect,
            CornerRadius::same(2),
            Color32::from_black_alpha(210),
        );
        painter.text(
            tc_rect.center(),
            egui::Align2::CENTER_CENTER,
            "01:00:14:08 • Frame 1420",
            FontId::monospace(9.5),
            Color32::from_rgb(80, 240, 140),
        );

        ui.add_space(10.0);

        // Center: Stereo Audio Peak Meters (L and R)
        ui.allocate_ui(Vec2::new(70.0, 146.0), |ui| {
            ui.label(RichText::new("AUDIO PEAK").size(9.5).color(Color32::GRAY));
            let (m_rect, _) = ui.allocate_exact_size(Vec2::new(60.0, 116.0), egui::Sense::hover());
            let p = ui.painter_at(m_rect);

            p.rect_filled(m_rect, CornerRadius::same(2), Color32::from_rgb(16, 20, 26));

            // Left Channel (-14 dB)
            let l_h = m_rect.height() * 0.72;
            let l_bar = Rect::from_min_size(
                Pos2::new(m_rect.min.x + 8.0, m_rect.max.y - l_h),
                Vec2::new(14.0, l_h),
            );
            p.rect_filled(
                l_bar,
                CornerRadius::same(1),
                Color32::from_rgb(80, 220, 120),
            );

            // Right Channel (-16 dB)
            let r_h = m_rect.height() * 0.68;
            let r_bar = Rect::from_min_size(
                Pos2::new(m_rect.min.x + 28.0, m_rect.max.y - r_h),
                Vec2::new(14.0, r_h),
            );
            p.rect_filled(
                r_bar,
                CornerRadius::same(1),
                Color32::from_rgb(80, 220, 120),
            );

            // Channel labels
            p.text(
                Pos2::new(m_rect.min.x + 15.0, m_rect.max.y - 8.0),
                egui::Align2::CENTER_CENTER,
                "L",
                FontId::proportional(8.5),
                Color32::WHITE,
            );
            p.text(
                Pos2::new(m_rect.min.x + 35.0, m_rect.max.y - 8.0),
                egui::Align2::CENTER_CENTER,
                "R",
                FontId::proportional(8.5),
                Color32::WHITE,
            );
        });

        ui.add_space(10.0);

        // Right: Telemetry & Stream Syntax
        ui.allocate_ui(Vec2::new(ui.available_width() - 10.0, 146.0), |ui| {
            ui.label(
                RichText::new("STREAM SYNTAX & ENCODER TELEMETRY")
                    .size(9.5)
                    .color(Color32::from_rgb(130, 145, 165))
                    .strong(),
            );
            egui::Grid::new("telemetry_grid")
                .num_columns(2)
                .spacing(Vec2::new(12.0, 3.0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Transcode Speed:")
                            .size(10.0)
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("59.94 fps (2.50x real-time throughput)")
                            .size(10.0)
                            .strong()
                            .color(Color32::from_rgb(90, 230, 130)),
                    );
                    ui.end_row();

                    ui.label(
                        RichText::new("Target Bitrate:")
                            .size(10.0)
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("4,820 kbps (H.264 High @ Level 4.2)")
                            .size(10.0)
                            .color(Color32::from_rgb(180, 215, 255)),
                    );
                    ui.end_row();

                    ui.label(
                        RichText::new("GOP Structure:")
                            .size(10.0)
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("Closed GOP (M=3, N=12 cadence, 2 B-frames)")
                            .size(10.0)
                            .color(Color32::from_rgb(220, 235, 255)),
                    );
                    ui.end_row();

                    ui.label(
                        RichText::new("Audio Priming:")
                            .size(10.0)
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("1024-sample elst edit list delay compensated")
                            .size(10.0)
                            .color(Color32::from_rgb(220, 235, 255)),
                    );
                    ui.end_row();

                    ui.label(
                        RichText::new("Container Atom:")
                            .size(10.0)
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("MP4 Faststart (moov header placed before mdat)")
                            .size(10.0)
                            .color(Color32::from_rgb(90, 230, 130)),
                    );
                    ui.end_row();

                    ui.label(
                        RichText::new("Estimated Fidelity:")
                            .size(10.0)
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("96.4 VMAF • 44.8 dB PSNR")
                            .size(10.0)
                            .color(Color32::from_rgb(140, 210, 255)),
                    );
                    ui.end_row();
                });
        });
    });
}

fn load_default_sample_jobs(engine: &mut QueueEngine, presets: &[EncodingPreset]) {
    let p_web = presets
        .iter()
        .find(|p| p.id.contains("web"))
        .cloned()
        .unwrap_or_default();

    let p_proxy = presets
        .iter()
        .find(|p| p.id.contains("proxy"))
        .cloned()
        .unwrap_or_default();

    let j1 = engine.add_job(
        PathBuf::from("/Users/zachshallbetter/Projects/Media/A001_C001.mov"),
        PathBuf::from("/Users/zachshallbetter/Downloads/A001_C001_proxy.mp4"),
        p_proxy,
        5,
    );
    let _ = engine.update_progress(j1, 0.62, 1488, 2400, 59.94);

    let j2 = engine.add_job(
        PathBuf::from("/Users/zachshallbetter/Projects/Media/Interview_B_Roll.mp4"),
        PathBuf::from("/Users/zachshallbetter/Downloads/Interview_B_Roll_web.mp4"),
        p_web,
        4,
    );
    let _ = engine.update_progress(j2, 0.15, 135, 900, 29.97);

    let _ = engine.add_job(
        PathBuf::from("/Users/zachshallbetter/Projects/Media/Final_Master_4K.mov"),
        PathBuf::from("/Users/zachshallbetter/Downloads/Final_Master_ProRes.mov"),
        presets.first().cloned().unwrap_or_default(),
        3,
    );
}
