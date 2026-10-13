//! Dedicated Transcoding Queue Workspace (MC-01 to MC-05)
//!
//! Provides the primary encoding pipeline workspace, supporting both empty-state
//! first-launch ingest affordances (MC-01) and populated active batch transcode operations (MC-02..MC-05).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use egui::{Color32, CornerRadius, FontId, Pos2, Rect, RichText, Stroke, Vec2};
use mediacraft_daemon::QueueEngine;
use mediacraft_protocol::{EncodingPreset, JobId, JobState, PauseReason};

pub struct QueueViewState {
    pub selected_job_id: Option<JobId>,
    pub sweep_phase: f32,
    pub selected_jobs: HashSet<JobId>,

    // MD-06: Encoding Error Details Modal
    pub show_error_modal: bool,
    pub error_modal_job_id: Option<JobId>,
    pub error_modal_copied: bool,

    // MD-07: Unsupported Codec / Container Modal
    pub show_unsupported_modal: bool,
    pub unsupported_filename: String,
    pub unsupported_fourcc: String,
    pub unsupported_container: String,
    pub unsupported_reason: String,

    // MD-10: Batch Edit Settings Modal
    pub show_batch_edit_modal: bool,
    pub batch_preset_idx: usize,
    pub batch_destination: String,
    pub batch_priority: u32,
    pub batch_overwrite_policy: usize,
    pub batch_faststart: bool,
}

impl Default for QueueViewState {
    fn default() -> Self {
        Self {
            selected_job_id: None,
            sweep_phase: 0.0,
            selected_jobs: HashSet::new(),

            show_error_modal: false,
            error_modal_job_id: None,
            error_modal_copied: false,

            show_unsupported_modal: false,
            unsupported_filename: "A009_C004_RED_Monstro_8K.r3d".to_string(),
            unsupported_fourcc: "R3D9".to_string(),
            unsupported_container: "REDCODE RAW Wavelet Container".to_string(),
            unsupported_reason: "Proprietary wavelet compression payload requires proprietary SDK or uncompressed demux pass-through. ISO/IEC 14496-12 atoms missing.".to_string(),

            show_batch_edit_modal: false,
            batch_preset_idx: 0,
            batch_destination: "/Users/zachshallbetter/Downloads/BatchOutputs/".to_string(),
            batch_priority: 5,
            batch_overwrite_policy: 1, // Auto-Rename (_v2)
            batch_faststart: true,
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
                ui.add_space((ui.available_width() - 340.0) * 0.5);
                let primary_style = crate::craft_theme::craft_icon_button_style();
                if craft_ui::buttons::IconButton::new(
                    "Load Sample Queue",
                    Vec2::new(210.0, 28.0),
                    &primary_style,
                )
                .show(ui, |ui, rect, _tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "＋ Load Sample Transcode Queue",
                        egui::FontId::proportional(11.5),
                        Color32::from_rgb(90, 200, 255),
                    );
                })
                .clicked()
                {
                    load_default_sample_jobs(engine, presets);
                }
                ui.add_space(8.0);
                if craft_ui::buttons::IconButton::new(
                    "Browse Files",
                    Vec2::new(120.0, 28.0),
                    &primary_style,
                )
                .show(ui, |ui, rect, tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "📁 Browse Files...",
                        egui::FontId::proportional(11.5),
                        tint,
                    );
                })
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

        let accent_style = crate::craft_theme::craft_accent_button_style();
        let normal_style = crate::craft_theme::craft_icon_button_style();
        let danger_style = crate::craft_theme::craft_danger_button_style();

        if is_paused {
            if craft_ui::buttons::IconButton::new(
                "Resume Queue",
                Vec2::new(110.0, 24.0),
                &accent_style,
            )
            .show(ui, |ui, rect, tint| {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "▶ Resume Queue",
                    egui::FontId::proportional(11.0),
                    tint,
                );
            })
            .clicked()
            {
                engine.resume_queue(PauseReason::UserRequested);
            }
        } else if is_running && status.active_jobs > 0 {
            if craft_ui::buttons::IconButton::new(
                "Pause Queue",
                Vec2::new(100.0, 24.0),
                &normal_style,
            )
            .show(ui, |ui, rect, _tint| {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "⏸ Pause Queue",
                    egui::FontId::proportional(11.0),
                    Color32::from_rgb(255, 190, 70),
                );
            })
            .clicked()
            {
                engine.pause_queue(PauseReason::UserRequested);
            }
        } else if craft_ui::buttons::IconButton::new(
            "Start Queue",
            Vec2::new(100.0, 24.0),
            &accent_style,
        )
        .show(ui, |ui, rect, tint| {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "▶ Start Queue",
                egui::FontId::proportional(11.0),
                tint,
            );
        })
        .clicked()
        {
            let _ = engine.poll_dispatch();
        }

        if craft_ui::buttons::IconButton::new("Stop Queue", Vec2::new(65.0, 24.0), &danger_style)
            .show(ui, |ui, rect, tint| {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "⏹ Stop",
                    egui::FontId::proportional(11.0),
                    tint,
                );
            })
            .clicked()
        {
            engine.pause_queue(PauseReason::UserRequested);
        }

        ui.add_space(6.0);
        if craft_ui::buttons::IconButton::new("Clear Done", Vec2::new(85.0, 24.0), &normal_style)
            .show(ui, |ui, rect, tint| {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "[-] Clear Done",
                    egui::FontId::proportional(11.0),
                    tint,
                );
            })
            .clicked()
        {
            engine.clear_completed();
        }

        if craft_ui::buttons::IconButton::new("Clear All", Vec2::new(75.0, 24.0), &normal_style)
            .show(ui, |ui, rect, tint| {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "🗑 Clear All",
                    egui::FontId::proportional(11.0),
                    tint,
                );
            })
            .clicked()
        {
            engine.clear_all();
        }

        ui.add_space(6.0);
        if craft_ui::buttons::IconButton::new(
            "Add Sample Job",
            Vec2::new(125.0, 24.0),
            &normal_style,
        )
        .show(ui, |ui, rect, _tint| {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "＋ Add Sample Job",
                egui::FontId::proportional(11.0),
                Color32::from_rgb(90, 200, 255),
            );
        })
        .clicked()
        {
            load_default_sample_jobs(engine, presets);
        }

        ui.add_space(4.0);
        if craft_ui::buttons::IconButton::new("Format Check", Vec2::new(140.0, 24.0), &normal_style)
            .show(ui, |ui, rect, _tint| {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "⚠️ Format Check (MD-07)",
                    egui::FontId::proportional(10.5),
                    Color32::from_rgb(255, 190, 70),
                );
            })
            .clicked()
        {
            state.show_unsupported_modal = true;
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

    // Batch operations toolbar bar if jobs are selected
    if !state.selected_jobs.is_empty() {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let accent_style = crate::craft_theme::craft_accent_button_style();
            let normal_style = crate::craft_theme::craft_icon_button_style();
            let danger_small = crate::craft_theme::craft_danger_button_style();

            ui.label(
                RichText::new(format!("● {} Job(s) Selected:", state.selected_jobs.len()))
                    .size(11.0)
                    .strong()
                    .color(Color32::from_rgb(90, 200, 255)),
            );

            if craft_ui::buttons::IconButton::new(
                "Batch Edit",
                Vec2::new(140.0, 22.0),
                &accent_style,
            )
            .show(ui, |ui, rect, tint| {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "⚙ Batch Edit... (MD-10)",
                    egui::FontId::proportional(10.5),
                    tint,
                );
            })
            .clicked()
            {
                state.show_batch_edit_modal = true;
            }

            if craft_ui::buttons::IconButton::new(
                "Retry Selected",
                Vec2::new(110.0, 22.0),
                &normal_style,
            )
            .show(ui, |ui, rect, tint| {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "⟳ Retry Selected",
                    egui::FontId::proportional(10.5),
                    tint,
                );
            })
            .clicked()
            {
                for &id in &state.selected_jobs {
                    engine.retry_job(id);
                }
            }

            if craft_ui::buttons::IconButton::new(
                "Remove Selected",
                Vec2::new(120.0, 22.0),
                &danger_small,
            )
            .show(ui, |ui, rect, tint| {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "🗑 Remove Selected",
                    egui::FontId::proportional(10.5),
                    tint,
                );
            })
            .clicked()
            {
                for &id in &state.selected_jobs {
                    engine.remove_job(id);
                }
                state.selected_jobs.clear();
            }
        });
        ui.add_space(2.0);
    }

    // 2. Queue Job Table Area
    let available_h = ui.available_height() - 200.0;
    egui::ScrollArea::vertical()
        .id_salt("populated_queue_table_scroll")
        .max_height(available_h.max(160.0))
        .show(ui, |ui| {
            egui::Grid::new("queue_jobs_grid")
                .striped(true)
                .num_columns(8)
                .spacing(Vec2::new(12.0, 8.0))
                .show(ui, |ui| {
                    let all_selected = !jobs.is_empty()
                        && jobs.iter().all(|j| state.selected_jobs.contains(&j.id));
                    let mut check_all = all_selected;
                    if ui.checkbox(&mut check_all, "").changed() {
                        if check_all {
                            for j in jobs {
                                state.selected_jobs.insert(j.id);
                            }
                        } else {
                            state.selected_jobs.clear();
                        }
                    }
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
                        // Checkbox
                        let mut is_checked = state.selected_jobs.contains(&job.id);
                        if ui.checkbox(&mut is_checked, "").changed() {
                            if is_checked {
                                state.selected_jobs.insert(job.id);
                            } else {
                                state.selected_jobs.remove(&job.id);
                            }
                        }

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
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(error_message)
                                            .size(10.0)
                                            .color(Color32::from_rgb(255, 80, 80)),
                                    );
                                    let danger_small =
                                        crate::craft_theme::craft_danger_button_style();
                                    if craft_ui::buttons::IconButton::new(
                                        "Error Details",
                                        Vec2::new(75.0, 18.0),
                                        &danger_small,
                                    )
                                    .show(ui, |ui, rect, tint| {
                                        ui.painter().text(
                                            rect.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "⚠️ (i) Details",
                                            egui::FontId::proportional(9.0),
                                            tint,
                                        );
                                    })
                                    .clicked()
                                    {
                                        state.error_modal_job_id = Some(job.id);
                                        state.show_error_modal = true;
                                    }
                                });
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

                        // Actions using craft_ui::buttons::IconButton
                        let small_style = crate::craft_theme::craft_small_button_style();
                        let danger_small = crate::craft_theme::craft_danger_button_style();
                        ui.horizontal(|ui| match job.state {
                            JobState::Running { .. } | JobState::Queued => {
                                if craft_ui::buttons::IconButton::new(
                                    "Cancel Job",
                                    Vec2::new(20.0, 20.0),
                                    &danger_small,
                                )
                                .show(ui, |ui, rect, tint| {
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "✕",
                                        egui::FontId::proportional(10.0),
                                        tint,
                                    );
                                })
                                .clicked()
                                {
                                    cancel_id = Some(job.id);
                                }
                            }
                            JobState::Failed { .. } | JobState::Cancelled => {
                                if craft_ui::buttons::IconButton::new(
                                    "Retry Job",
                                    Vec2::new(20.0, 20.0),
                                    &small_style,
                                )
                                .show(ui, |ui, rect, tint| {
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "⟳",
                                        egui::FontId::proportional(10.0),
                                        tint,
                                    );
                                })
                                .clicked()
                                {
                                    retry_id = Some(job.id);
                                }
                                if craft_ui::buttons::IconButton::new(
                                    "Remove Job",
                                    Vec2::new(20.0, 20.0),
                                    &danger_small,
                                )
                                .show(ui, |ui, rect, tint| {
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "🗑",
                                        egui::FontId::proportional(10.0),
                                        tint,
                                    );
                                })
                                .clicked()
                                {
                                    remove_id = Some(job.id);
                                }
                            }
                            JobState::Completed { .. } => {
                                if craft_ui::buttons::IconButton::new(
                                    "Remove Job",
                                    Vec2::new(20.0, 20.0),
                                    &small_style,
                                )
                                .show(ui, |ui, rect, tint| {
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "🗑",
                                        egui::FontId::proportional(10.0),
                                        tint,
                                    );
                                })
                                .clicked()
                                {
                                    remove_id = Some(job.id);
                                }
                            }
                            JobState::Paused { .. } => {
                                if craft_ui::buttons::IconButton::new(
                                    "Resume Job",
                                    Vec2::new(20.0, 20.0),
                                    &small_style,
                                )
                                .show(ui, |ui, rect, tint| {
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "▶",
                                        egui::FontId::proportional(10.0),
                                        tint,
                                    );
                                })
                                .clicked()
                                {
                                    engine.resume_queue(PauseReason::UserRequested);
                                }
                                if craft_ui::buttons::IconButton::new(
                                    "Cancel Job",
                                    Vec2::new(20.0, 20.0),
                                    &danger_small,
                                )
                                .show(ui, |ui, rect, tint| {
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "✕",
                                        egui::FontId::proportional(10.0),
                                        tint,
                                    );
                                })
                                .clicked()
                                {
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

    // MD-06: Encoding Error Details Modal
    render_encoding_error_modal(ui, state, engine);

    // MD-07: Unsupported Codec / Container Modal
    render_unsupported_format_modal(ui, state, engine, presets);

    // MD-10: Batch Edit Settings Modal
    render_batch_edit_modal(ui, state, engine, presets);
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
        p_web.clone(),
        4,
    );
    let _ = engine.update_progress(j2, 0.15, 135, 900, 29.97);

    let _ = engine.add_job(
        PathBuf::from("/Users/zachshallbetter/Projects/Media/Final_Master_4K.mov"),
        PathBuf::from("/Users/zachshallbetter/Downloads/Final_Master_ProRes.mov"),
        presets.first().cloned().unwrap_or_default(),
        3,
    );

    let j4 = engine.add_job(
        PathBuf::from("/Users/zachshallbetter/Projects/Media/Drone_Aerial_009_CorruptMoov.mov"),
        PathBuf::from("/Users/zachshallbetter/Downloads/Drone_Aerial_009_web.mp4"),
        p_web,
        2,
    );
    let _ = engine.fail_job(
        j4,
        "ERR_BITSTREAM_CORRUPT_MOOV: Faststart header space overflow (+142KB). Sample table size mismatch.".to_string(),
    );

    let j5 = engine.add_job(
        PathBuf::from("/Users/zachshallbetter/Projects/Media/Podcast_Audio_Track.wav"),
        PathBuf::from("/Users/zachshallbetter/Downloads/Podcast_Audio_Master.m4a"),
        presets
            .iter()
            .find(|p| p.id.contains("audio"))
            .cloned()
            .unwrap_or_default(),
        1,
    );
    let _ = engine.complete_job(j5, 4.82, 12_480_900);
}

fn render_encoding_error_modal(
    ui: &mut egui::Ui,
    state: &mut QueueViewState,
    engine: &mut QueueEngine,
) {
    if !state.show_error_modal {
        return;
    }

    let job_opt = state.error_modal_job_id.and_then(|id| engine.get_job(id));
    let job_id_text = match state.error_modal_job_id {
        Some(JobId(n)) => format!("Job #{n}"),
        None => "Unknown Job".to_string(),
    };
    let src_text = job_opt
        .as_ref()
        .map(|j| j.source_path.to_string_lossy().to_string())
        .unwrap_or_else(|| "/Volumes/ProductionRAID/Drone_Aerial_009_CorruptMoov.mov".to_string());
    let dest_text = job_opt
        .as_ref()
        .map(|j| j.output_path.to_string_lossy().to_string())
        .unwrap_or_else(|| "/Volumes/Deliveries/Web_ProRes/Drone_Aerial_009_web.mp4".to_string());
    let preset_text = job_opt
        .as_ref()
        .map(|j| j.preset.name.clone())
        .unwrap_or_else(|| "Match Source - High Bitrate (H.264/AAC)".to_string());

    let mut close = false;
    let mut retry_with_fix = false;

    egui::Window::new("Encoding Error Details (MD-06)")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([580.0, 480.0])
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("⚠️ Transcode Pipeline Failure Diagnostics")
                        .size(13.5)
                        .strong()
                        .color(Color32::from_rgb(255, 90, 90)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let pill_rect = ui
                        .allocate_exact_size(Vec2::new(145.0, 20.0), egui::Sense::hover())
                        .0;
                    ui.painter_at(pill_rect).rect_filled(
                        pill_rect,
                        CornerRadius::same(3),
                        Color32::from_rgb(60, 20, 20),
                    );
                    ui.painter_at(pill_rect).text(
                        pill_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "STAGE: MUXER_FASTSTART",
                        FontId::monospace(9.0),
                        Color32::from_rgb(255, 120, 120),
                    );
                });
            });

            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "{job_id_text} failed during container atom finalization."
                ))
                .size(10.5)
                .color(Color32::GRAY),
            );

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            // Job Metadata Grid
            egui::Grid::new("error_modal_meta_grid")
                .num_columns(2)
                .spacing(Vec2::new(10.0, 4.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("Source:").strong().size(10.5));
                    ui.label(
                        RichText::new(&src_text)
                            .size(10.5)
                            .color(Color32::from_rgb(200, 220, 245)),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Target:").strong().size(10.5));
                    ui.label(
                        RichText::new(&dest_text)
                            .size(10.5)
                            .color(Color32::from_rgb(180, 200, 220)),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Preset:").strong().size(10.5));
                    ui.label(
                        RichText::new(&preset_text)
                            .size(10.5)
                            .color(Color32::from_rgb(130, 190, 255)),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Error Code:").strong().size(10.5));
                    ui.label(
                        RichText::new("ERR_BITSTREAM_CORRUPT_MOOV (0x8004101A)")
                            .size(10.5)
                            .monospace()
                            .strong()
                            .color(Color32::from_rgb(255, 100, 100)),
                    );
                    ui.end_row();
                });

            ui.add_space(8.0);

            // Diagnostic Log View
            ui.label(
                RichText::new("PIPELINE STACK TRACE & LOGS")
                    .size(10.0)
                    .strong()
                    .color(Color32::GRAY),
            );
            let log_text = "[14:22:01.102] Pipeline worker #4 spawned (Apple Silicon VideoToolbox M3 Max)\n\
                            [14:22:01.150] Demuxing QuickTime container: 2400 video frames, 2 audio streams\n\
                            [14:22:01.890] Transcoding H.264 video track 3840x2160 @ 23.976 fps (High 4:2:2 @ Level 5.1)\n\
                            [14:22:24.410] Video transcode finished: 2400 frames encoded in 23.26s (103.1 fps)\n\
                            [14:22:24.450] Muxing MP4 atoms: ftyp, moov, mdat\n\
                            [14:22:24.510] ERROR: Faststart moov relocation failed: sample chunk offset exceeds 32-bit stco atom range (offset > 4,294,967,295 bytes).\n\
                            [14:22:24.512] FATAL: Muxer aborted to avoid writing truncated file. Cleaned up temporary write buffer.\n";

            let (log_rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), 95.0),
                egui::Sense::hover(),
            );
            let painter = ui.painter_at(log_rect);
            painter.rect_filled(log_rect, CornerRadius::same(4), Color32::from_rgb(12, 14, 18));
            painter.rect_stroke(
                log_rect,
                CornerRadius::same(4),
                Stroke::new(1.0, Color32::from_rgb(45, 25, 25)),
                egui::StrokeKind::Inside,
            );

            let mut log_child =
                ui.new_child(egui::UiBuilder::new().max_rect(log_rect.shrink(8.0)));
            egui::ScrollArea::vertical()
                .id_salt("error_modal_log_scroll")
                .show(&mut log_child, |ui| {
                    ui.label(
                        RichText::new(log_text)
                            .size(9.5)
                            .monospace()
                            .color(Color32::from_rgb(220, 180, 180)),
                    );
                });

            ui.add_space(8.0);

            // Suggested Remediation Card
            let (rem_rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), 65.0),
                egui::Sense::hover(),
            );
            let p_rem = ui.painter_at(rem_rect);
            p_rem.rect_filled(rem_rect, CornerRadius::same(4), Color32::from_rgb(16, 26, 22));
            p_rem.rect_stroke(
                rem_rect,
                CornerRadius::same(4),
                Stroke::new(1.0, Color32::from_rgb(30, 70, 45)),
                egui::StrokeKind::Inside,
            );

            let mut rem_child =
                ui.new_child(egui::UiBuilder::new().max_rect(rem_rect.shrink(8.0)));
            rem_child.vertical(|ui| {
                ui.label(
                    RichText::new("💡 RECOMMENDED REMEDIATION")
                        .size(10.0)
                        .strong()
                        .color(Color32::from_rgb(90, 230, 130)),
                );
                ui.label(
                    RichText::new(
                        "1. Upgrade container atom table to 64-bit chunk offsets (co64 atom).\n\
                         2. Switch to Software Fallback Muxer with extended memory buffer allocation.\n\
                         3. Disable in-place faststart or reserve 1MB moov padding before transcode.",
                    )
                    .size(9.5)
                    .color(Color32::from_rgb(200, 240, 215)),
                );
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            // Bottom Buttons
            ui.horizontal(|ui| {
                let normal_style = crate::craft_theme::craft_icon_button_style();
                let accent_style = crate::craft_theme::craft_accent_button_style();

                let copy_label = if state.error_modal_copied {
                    "✓ Diagnostics Copied!"
                } else {
                    "📋 Copy Diagnostics"
                };
                if craft_ui::buttons::IconButton::new(
                    "Copy Diagnostics",
                    Vec2::new(140.0, 26.0),
                    &normal_style,
                )
                .show(ui, |ui, rect, tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        copy_label,
                        FontId::proportional(11.0),
                        tint,
                    );
                })
                .clicked()
                {
                    state.error_modal_copied = true;
                    ui.ctx().copy_text(format!(
                        "MediaCraft Failure Report:\nSource: {src_text}\nTarget: {dest_text}\nLogs:\n{log_text}"
                    ));
                }

                ui.add_space(8.0);
                if craft_ui::buttons::IconButton::new(
                    "Apply Fix and Retry",
                    Vec2::new(180.0, 26.0),
                    &accent_style,
                )
                .show(ui, |ui, rect, tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "🔄 Apply Fix & Retry Job",
                        FontId::proportional(11.0),
                        tint,
                    );
                })
                .clicked()
                {
                    retry_with_fix = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if craft_ui::buttons::IconButton::new(
                        "Close",
                        Vec2::new(75.0, 26.0),
                        &normal_style,
                    )
                    .show(ui, |ui, rect, tint| {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Close",
                            FontId::proportional(11.0),
                            tint,
                        );
                    })
                    .clicked()
                    {
                        close = true;
                    }
                });
            });
        });

    if retry_with_fix {
        if let Some(id) = state.error_modal_job_id {
            engine.retry_job(id);
        }
        state.show_error_modal = false;
        state.error_modal_copied = false;
    }
    if close {
        state.show_error_modal = false;
        state.error_modal_copied = false;
    }
}

fn render_unsupported_format_modal(
    ui: &mut egui::Ui,
    state: &mut QueueViewState,
    engine: &mut QueueEngine,
    presets: &[EncodingPreset],
) {
    if !state.show_unsupported_modal {
        return;
    }

    let mut close = false;
    let mut do_transcode = false;

    egui::Window::new("Unsupported Format Detected (MD-07)")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([540.0, 390.0])
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("⚠️ Unsupported Codec / Container")
                        .size(13.5)
                        .strong()
                        .color(Color32::from_rgb(255, 190, 70)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let pill_rect = ui
                        .allocate_exact_size(Vec2::new(115.0, 20.0), egui::Sense::hover())
                        .0;
                    ui.painter_at(pill_rect).rect_filled(
                        pill_rect,
                        CornerRadius::same(3),
                        Color32::from_rgb(50, 40, 15),
                    );
                    ui.painter_at(pill_rect).text(
                        pill_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "NON-STANDARD",
                        FontId::monospace(9.0),
                        Color32::from_rgb(255, 200, 80),
                    );
                });
            });

            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "The selected media file contains a container or stream payload without an active open decoder.",
                )
                .size(10.5)
                .color(Color32::GRAY),
            );

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            // Technical Breakdown
            egui::Grid::new("unsupported_modal_grid")
                .num_columns(2)
                .spacing(Vec2::new(12.0, 4.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("Filename:").strong().size(10.5));
                    ui.label(
                        RichText::new(&state.unsupported_filename)
                            .size(10.5)
                            .color(Color32::from_rgb(210, 230, 255)),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Container Box:").strong().size(10.5));
                    ui.label(
                        RichText::new(&state.unsupported_container)
                            .size(10.5)
                            .color(Color32::from_rgb(180, 200, 220)),
                    );
                    ui.end_row();

                    ui.label(RichText::new("FourCC Codec:").strong().size(10.5));
                    ui.label(
                        RichText::new(&state.unsupported_fourcc)
                            .size(10.5)
                            .monospace()
                            .color(Color32::from_rgb(255, 200, 100)),
                    );
                    ui.end_row();
                });

            ui.add_space(8.0);

            // Diagnostic Box
            let (diag_rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), 65.0),
                egui::Sense::hover(),
            );
            let painter = ui.painter_at(diag_rect);
            painter.rect_filled(diag_rect, CornerRadius::same(4), Color32::from_rgb(18, 20, 26));
            painter.rect_stroke(
                diag_rect,
                CornerRadius::same(4),
                Stroke::new(1.0, Color32::from_rgb(45, 50, 65)),
                egui::StrokeKind::Inside,
            );

            let mut diag_child =
                ui.new_child(egui::UiBuilder::new().max_rect(diag_rect.shrink(8.0)));
            diag_child.vertical(|ui| {
                ui.label(
                    RichText::new("DIAGNOSTIC ANALYSIS")
                        .size(9.5)
                        .strong()
                        .color(Color32::GRAY),
                );
                ui.label(
                    RichText::new(&state.unsupported_reason)
                        .size(10.0)
                        .color(Color32::from_rgb(200, 210, 230)),
                );
            });

            ui.add_space(10.0);
            ui.label(
                RichText::new("SUGGESTED WORKAROUNDS & ACTIONS")
                    .size(10.0)
                    .strong()
                    .color(Color32::from_rgb(90, 200, 255)),
            );

            let cards = [
                (
                    "1. Transcode via Universal Mezzanine Pipeline",
                    "Convert to Apple ProRes 422 HQ mezzanine for maximum quality preservation.",
                ),
                (
                    "2. Pass-Through Demux Elementary Bitstreams",
                    "Extract raw compressed video/audio elementary streams without re-encoding.",
                ),
                (
                    "3. Add as Unrendered Catalog Placeholder",
                    "Track file provenance, checksums, and tags in MediaCraft catalog without raster preview.",
                ),
            ];
            for (title, desc) in cards {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("•")
                            .color(Color32::from_rgb(90, 200, 255))
                            .strong(),
                    );
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(title)
                                .size(10.5)
                                .strong()
                                .color(Color32::from_rgb(230, 240, 255)),
                        );
                        ui.label(RichText::new(desc).size(9.5).color(Color32::GRAY));
                    });
                });
                ui.add_space(2.0);
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                let accent_style = crate::craft_theme::craft_accent_button_style();
                let normal_style = crate::craft_theme::craft_icon_button_style();

                if craft_ui::buttons::IconButton::new(
                    "Transcode to ProRes",
                    Vec2::new(180.0, 26.0),
                    &accent_style,
                )
                .show(ui, |ui, rect, tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "▶ Transcode to ProRes 422",
                        FontId::proportional(11.0),
                        tint,
                    );
                })
                .clicked()
                {
                    do_transcode = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if craft_ui::buttons::IconButton::new(
                        "Dismiss",
                        Vec2::new(75.0, 26.0),
                        &normal_style,
                    )
                    .show(ui, |ui, rect, tint| {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Dismiss",
                            FontId::proportional(11.0),
                            tint,
                        );
                    })
                    .clicked()
                    {
                        close = true;
                    }
                });
            });
        });

    if do_transcode {
        let preset = presets
            .iter()
            .find(|p| p.id.contains("prores"))
            .cloned()
            .unwrap_or_default();
        let src = PathBuf::from(format!("/Sources/{}", state.unsupported_filename));
        let dest = PathBuf::from(format!(
            "/Exports/{}_mezzanine.mov",
            state.unsupported_filename
        ));
        let _ = engine.add_job(src, dest, preset, 6);
        state.show_unsupported_modal = false;
    }
    if close {
        state.show_unsupported_modal = false;
    }
}

fn render_batch_edit_modal(
    ui: &mut egui::Ui,
    state: &mut QueueViewState,
    engine: &mut QueueEngine,
    presets: &[EncodingPreset],
) {
    if !state.show_batch_edit_modal {
        return;
    }

    let mut close = false;
    let mut apply = false;
    let count = state.selected_jobs.len();

    egui::Window::new("Batch Edit Settings (MD-10)")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .fixed_size([560.0, 430.0])
        .show(ui.ctx(), |ui| {
            ui.horizontal(|ui| {
                ui.heading(
                    RichText::new("⚙️ Batch Queue Settings Configuration")
                        .size(13.5)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let pill_rect = ui
                        .allocate_exact_size(Vec2::new(120.0, 20.0), egui::Sense::hover())
                        .0;
                    ui.painter_at(pill_rect).rect_filled(
                        pill_rect,
                        CornerRadius::same(3),
                        Color32::from_rgb(25, 45, 70),
                    );
                    ui.painter_at(pill_rect).text(
                        pill_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{count} JOBS SELECTED"),
                        FontId::monospace(9.0),
                        Color32::from_rgb(100, 190, 255),
                    );
                });
            });

            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Modify encoding preset, destination folder, priority, and collision rules across all selected jobs simultaneously.",
                )
                .size(10.5)
                .color(Color32::GRAY),
            );

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            // Form Fields
            egui::Grid::new("batch_edit_form_grid")
                .num_columns(2)
                .spacing(Vec2::new(14.0, 10.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("Encoding Preset:").strong().size(11.0));
                    egui::ComboBox::from_id_salt("batch_preset_combo")
                        .width(360.0)
                        .selected_text(
                            presets
                                .get(state.batch_preset_idx)
                                .map(|p| p.name.as_str())
                                .unwrap_or("Select preset..."),
                        )
                        .show_ui(ui, |ui| {
                            for (idx, p) in presets.iter().enumerate() {
                                ui.selectable_value(&mut state.batch_preset_idx, idx, &p.name);
                            }
                        });
                    ui.end_row();

                    ui.label(RichText::new("Destination Folder:").strong().size(11.0));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut state.batch_destination)
                                .desired_width(280.0),
                        );
                        let small_style = crate::craft_theme::craft_small_button_style();
                        if craft_ui::buttons::IconButton::new(
                            "Browse Dest",
                            Vec2::new(70.0, 20.0),
                            &small_style,
                        )
                        .show(ui, |ui, rect, tint| {
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "Browse...",
                                FontId::proportional(10.0),
                                tint,
                            );
                        })
                        .clicked()
                        {
                            state.batch_destination =
                                "/Volumes/ProductionRAID/Deliveries_Batch/".to_string();
                        }
                    });
                    ui.end_row();

                    ui.label(RichText::new("Queue Priority:").strong().size(11.0));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut state.batch_priority, 8, "High (Urgent)");
                        ui.radio_value(&mut state.batch_priority, 5, "Normal (Default)");
                        ui.radio_value(&mut state.batch_priority, 2, "Low (Background)");
                    });
                    ui.end_row();

                    ui.label(RichText::new("Collision Policy:").strong().size(11.0));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut state.batch_overwrite_policy, 1, "Auto-Rename (_v2)");
                        ui.radio_value(&mut state.batch_overwrite_policy, 0, "Overwrite");
                        ui.radio_value(&mut state.batch_overwrite_policy, 2, "Skip");
                    });
                    ui.end_row();

                    ui.label(RichText::new("Web Optimization:").strong().size(11.0));
                    ui.checkbox(
                        &mut state.batch_faststart,
                        "Relocate Moov Atom to Start of File (Faststart)",
                    );
                    ui.end_row();
                });

            ui.add_space(14.0);
            ui.separator();
            ui.add_space(10.0);

            // Action Buttons
            ui.horizontal(|ui| {
                let accent_style = crate::craft_theme::craft_accent_button_style();
                let normal_style = crate::craft_theme::craft_icon_button_style();

                if craft_ui::buttons::IconButton::new(
                    "Apply Batch",
                    Vec2::new(170.0, 26.0),
                    &accent_style,
                )
                .show(ui, |ui, rect, tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("✓ Apply to {count} Jobs"),
                        FontId::proportional(11.0),
                        tint,
                    );
                })
                .clicked()
                {
                    apply = true;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if craft_ui::buttons::IconButton::new(
                        "Cancel",
                        Vec2::new(75.0, 26.0),
                        &normal_style,
                    )
                    .show(ui, |ui, rect, tint| {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Cancel",
                            FontId::proportional(11.0),
                            tint,
                        );
                    })
                    .clicked()
                    {
                        close = true;
                    }
                });
            });
        });

    if apply {
        let chosen_preset = presets.get(state.batch_preset_idx).cloned();
        let dest_base = PathBuf::from(&state.batch_destination);
        for &id in &state.selected_jobs {
            let cur_job = engine.get_job(id);
            let new_dest = cur_job.map(|j| {
                let fname = j.output_path.file_name().unwrap_or_default();
                dest_base.join(fname)
            });
            engine.update_job_settings(
                id,
                chosen_preset.clone(),
                new_dest,
                Some(state.batch_priority),
            );
        }
        state.show_batch_edit_modal = false;
    }
    if close {
        state.show_batch_edit_modal = false;
    }
}
