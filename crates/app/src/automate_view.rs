//! Automation & Watch Folders Workspace (MC-13, MC-14, MC-15)
//!
//! Provides directory monitoring, automated ingest rules, batch transcode pipelines,
//! and live execution diagnostics.

use egui::{Color32, CornerRadius, FontId, Pos2, Rect, RichText, Stroke, Vec2};

pub struct AutomateViewState {
    pub is_monitoring_active: bool,
    pub selected_rule_idx: usize,
    pub search_query: String,
}

impl Default for AutomateViewState {
    fn default() -> Self {
        Self {
            is_monitoring_active: true,
            selected_rule_idx: 0,
            search_query: String::new(),
        }
    }
}

pub struct WatchFolderRuleModel {
    pub path: String,
    pub preset: String,
    pub filter: String,
    pub target: String,
    pub post_action: String,
    pub stats: String,
    pub is_active: bool,
}

pub fn render_automate_workspace(
    ui: &mut egui::Ui,
    state: &mut AutomateViewState,
    watch_engine: &mut mediacraft_daemon::watch_folder::WatchFolderEngine,
    queue: &mut mediacraft_daemon::QueueEngine,
    presets: &[mediacraft_protocol::EncodingPreset],
) {
    // 1. Top Automation Toolbar
    ui.horizontal(|ui| {
        let normal_style = crate::craft_theme::craft_icon_button_style();
        let accent_style = crate::craft_theme::craft_accent_button_style();

        if state.is_monitoring_active {
            if craft_ui::buttons::IconButton::new(
                "Pause All Watchers",
                Vec2::new(140.0, 24.0),
                &normal_style,
            )
            .show(ui, |ui, rect, _tint| {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "⏸ Pause All Watchers",
                    egui::FontId::proportional(11.0),
                    Color32::from_rgb(255, 190, 70),
                );
            })
            .clicked()
            {
                state.is_monitoring_active = false;
                for r in watch_engine.list_rules_mut() {
                    r.active = false;
                }
            }
        } else if craft_ui::buttons::IconButton::new(
            "Resume All Watchers",
            Vec2::new(145.0, 24.0),
            &accent_style,
        )
        .show(ui, |ui, rect, tint| {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "▶ Resume All Watchers",
                egui::FontId::proportional(11.0),
                tint,
            );
        })
        .clicked()
        {
            state.is_monitoring_active = true;
            for r in watch_engine.list_rules_mut() {
                r.active = true;
            }
        }

        ui.add_space(4.0);
        if craft_ui::buttons::IconButton::new(
            "Add Watch Folder",
            Vec2::new(135.0, 24.0),
            &normal_style,
        )
        .show(ui, |ui, rect, _tint| {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "＋ Add Watch Folder",
                egui::FontId::proportional(11.0),
                Color32::from_rgb(90, 200, 255),
            );
        })
        .clicked()
        {
            let count = watch_engine.list_rules().len() + 1;
            watch_engine.add_rule(mediacraft_daemon::watch_folder::WatchFolderRule {
                id: format!("wf-{count}"),
                name: format!("Camera Ingest Watcher #{count}"),
                watch_dir: std::path::PathBuf::from(
                    "/Users/zachshallbetter/Movies/WatchFolders/Ingest",
                ),
                output_dir: std::path::PathBuf::from(
                    "/Users/zachshallbetter/Movies/WatchFolders/Deliveries",
                ),
                preset_id: "match-source-hq".to_string(),
                active: true,
                file_filter_exts: vec!["mov".to_string(), "mp4".to_string(), "mxf".to_string()],
                min_stability_duration: std::time::Duration::from_secs(2),
            });
        }

        if craft_ui::buttons::IconButton::new(
            "Trigger Scan Now",
            Vec2::new(125.0, 24.0),
            &normal_style,
        )
        .show(ui, |ui, rect, tint| {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "⟳ Trigger Scan Now",
                egui::FontId::proportional(11.0),
                tint,
            );
        })
        .clicked()
        {
            let _ = watch_engine.scan_and_enqueue(queue, presets);
        }

        ui.add_space(8.0);
        ui.label(RichText::new("🔍").size(11.0).color(Color32::GRAY));
        ui.add(
            egui::TextEdit::singleline(&mut state.search_query)
                .hint_text("Filter automation rules...")
                .desired_width(180.0),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let active_count = watch_engine
                .list_rules()
                .iter()
                .filter(|r| r.active)
                .count();
            let (status_text, status_col) = if state.is_monitoring_active && active_count > 0 {
                (
                    format!("● WATCHERS RUNNING ({active_count} Active)"),
                    Color32::from_rgb(90, 230, 130),
                )
            } else {
                (
                    "○ WATCHERS SUSPENDED".to_string(),
                    Color32::from_rgb(255, 180, 50),
                )
            };

            ui.label(
                RichText::new(status_text)
                    .size(11.0)
                    .strong()
                    .color(status_col),
            );
        });
    });

    ui.add_space(4.0);
    ui.separator();

    // 2. Main Body: Split into Watch Folder Rules (Left / Center) and Recent Execution Log (Right / Bottom)
    let available_h = ui.available_height() - 170.0;

    egui::ScrollArea::vertical()
        .id_salt("automate_cards_scroll")
        .max_height(available_h.max(200.0))
        .show(ui, |ui| {
            ui.label(
                RichText::new("MONITORED WATCH FOLDERS & INGEST PIPELINES")
                    .size(10.5)
                    .color(Color32::from_rgb(130, 145, 165))
                    .strong(),
            );
            ui.add_space(6.0);

            for (idx, rule) in watch_engine.list_rules().iter().enumerate() {
                let card_model = WatchFolderRuleModel {
                    path: rule.watch_dir.to_string_lossy().to_string(),
                    preset: rule.preset_id.clone(),
                    filter: if rule.file_filter_exts.is_empty() {
                        "All Formats".to_string()
                    } else {
                        rule.file_filter_exts.join(", ")
                    },
                    target: rule.output_dir.to_string_lossy().to_string(),
                    post_action: "Generate SHA-256 Receipt • Auto Enqueue".to_string(),
                    stats: format!(
                        "Active: {} • Stability: {:.1}s",
                        rule.active,
                        rule.min_stability_duration.as_secs_f32()
                    ),
                    is_active: rule.active,
                };

                if render_watch_folder_card(ui, &card_model, state.selected_rule_idx == idx) {
                    state.selected_rule_idx = idx;
                }
                ui.add_space(8.0);
            }
        });

    ui.separator();

    // 3. Automation Event Log (MC-15 / MP-09)
    ui.label(
        RichText::new("AUTOMATION EXECUTION HISTORY & INGEST EVENT LOG")
            .size(10.5)
            .color(Color32::from_rgb(130, 145, 165))
            .strong(),
    );
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .id_salt("automate_history_scroll")
        .max_height(140.0)
        .show(ui, |ui| {
            egui::Grid::new("automate_log_grid")
                .striped(true)
                .num_columns(5)
                .spacing(Vec2::new(14.0, 6.0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Timestamp")
                            .size(10.5)
                            .strong()
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("Detected Source")
                            .size(10.5)
                            .strong()
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("Matched Pipeline")
                            .size(10.5)
                            .strong()
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("Output Target")
                            .size(10.5)
                            .strong()
                            .color(Color32::GRAY),
                    );
                    ui.label(
                        RichText::new("Result")
                            .size(10.5)
                            .strong()
                            .color(Color32::GRAY),
                    );
                    ui.end_row();

                    let jobs = queue.list_jobs();
                    if jobs.is_empty() {
                        ui.label(
                            RichText::new("--:--:--")
                                .size(10.0)
                                .monospace()
                                .color(Color32::GRAY),
                        );
                        ui.label(
                            RichText::new("No recent ingest events")
                                .size(10.5)
                                .italics(),
                        );
                        ui.label(RichText::new("Waiting for watch folder triggers...").size(10.0));
                        ui.label(RichText::new("-").size(10.0));
                        ui.label(RichText::new("Idle").size(10.0).color(Color32::GRAY));
                        ui.end_row();
                    } else {
                        for j in jobs.iter().rev().take(8) {
                            let src_name = j
                                .source_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("source");
                            let out_name = j
                                .output_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("output");
                            let (state_str, state_col) = match j.state {
                                mediacraft_protocol::JobState::Completed { .. } => {
                                    ("✓ Completed & QC Verified", Color32::from_rgb(80, 230, 120))
                                }
                                mediacraft_protocol::JobState::Running { .. } => {
                                    ("⚡ Transcoding", Color32::from_rgb(80, 190, 255))
                                }
                                mediacraft_protocol::JobState::Failed { .. } => {
                                    ("✕ Failed", Color32::from_rgb(255, 90, 90))
                                }
                                _ => ("Queued", Color32::from_rgb(220, 200, 100)),
                            };

                            ui.label(
                                RichText::new("Today Auto")
                                    .size(10.0)
                                    .monospace()
                                    .color(Color32::from_rgb(160, 175, 195)),
                            );
                            ui.label(RichText::new(src_name).size(10.5).strong());
                            ui.label(
                                RichText::new(&j.preset.name)
                                    .size(10.0)
                                    .color(Color32::from_rgb(180, 215, 255)),
                            );
                            ui.label(
                                RichText::new(out_name)
                                    .size(10.0)
                                    .color(Color32::from_rgb(190, 200, 215)),
                            );
                            ui.label(
                                RichText::new(state_str)
                                    .size(10.0)
                                    .color(state_col)
                                    .strong(),
                            );
                            ui.end_row();
                        }
                    }
                });
        });
}

fn render_watch_folder_card(
    ui: &mut egui::Ui,
    rule: &WatchFolderRuleModel,
    is_selected: bool,
) -> bool {
    let card_w = (ui.available_width() - 8.0).max(280.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, 96.0), egui::Sense::click());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, CornerRadius::same(6), Color32::from_rgb(22, 26, 34));
    let stroke_color = if is_selected {
        Color32::from_rgb(80, 190, 255)
    } else if rule.is_active {
        Color32::from_rgb(45, 60, 80)
    } else {
        Color32::from_rgb(40, 44, 52)
    };
    painter.rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(if is_selected { 1.5 } else { 1.0 }, stroke_color),
        egui::StrokeKind::Inside,
    );

    // Active status bar on left edge
    let bar_rect = Rect::from_min_max(rect.min, Pos2::new(rect.min.x + 4.0, rect.max.y));
    let bar_color = if rule.is_active {
        Color32::from_rgb(90, 230, 130)
    } else {
        Color32::from_rgb(255, 190, 70)
    };
    painter.rect_filled(bar_rect, CornerRadius::same(2), bar_color);

    // Card Content
    let text_x = rect.min.x + 14.0;
    painter.text(
        Pos2::new(text_x, rect.min.y + 10.0),
        egui::Align2::LEFT_TOP,
        &rule.path,
        FontId::proportional(12.0),
        Color32::from_rgb(240, 245, 255),
    );

    painter.text(
        Pos2::new(text_x, rect.min.y + 30.0),
        egui::Align2::LEFT_TOP,
        format!("Preset: {} • Filter: {}", rule.preset, rule.filter),
        FontId::proportional(10.5),
        Color32::from_rgb(140, 195, 255),
    );

    painter.text(
        Pos2::new(text_x, rect.min.y + 48.0),
        egui::Align2::LEFT_TOP,
        format!("Destination: {} • Post: {}", rule.target, rule.post_action),
        FontId::proportional(10.0),
        Color32::from_rgb(170, 185, 205),
    );

    painter.text(
        Pos2::new(text_x, rect.min.y + 68.0),
        egui::Align2::LEFT_TOP,
        &rule.stats,
        FontId::proportional(9.5),
        Color32::GRAY,
    );

    resp.clicked()
}
