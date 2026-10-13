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

pub fn render_automate_workspace(ui: &mut egui::Ui, state: &mut AutomateViewState) {
    // 1. Top Automation Toolbar
    ui.horizontal(|ui| {
        if state.is_monitoring_active {
            if ui
                .button(
                    RichText::new("⏸ Pause All Watchers")
                        .size(11.5)
                        .color(Color32::from_rgb(255, 190, 70)),
                )
                .clicked()
            {
                state.is_monitoring_active = false;
            }
        } else if ui
            .button(
                RichText::new("▶ Resume All Watchers")
                    .size(11.5)
                    .strong()
                    .color(Color32::from_rgb(90, 230, 130)),
            )
            .clicked()
        {
            state.is_monitoring_active = true;
        }

        let _ = ui
            .button(
                RichText::new("＋ Add Watch Folder")
                    .size(11.5)
                    .strong()
                    .color(Color32::from_rgb(90, 200, 255)),
            )
            .clicked();

        let _ = ui
            .button(RichText::new("⟳ Trigger Scan Now").size(11.5))
            .clicked();

        ui.add_space(8.0);
        ui.label(RichText::new("🔍").size(11.0).color(Color32::GRAY));
        ui.add(
            egui::TextEdit::singleline(&mut state.search_query)
                .hint_text("Filter automation rules...")
                .desired_width(180.0),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (status_text, status_col) = if state.is_monitoring_active {
                (
                    "● WATCHERS RUNNING (2 Active)",
                    Color32::from_rgb(90, 230, 130),
                )
            } else {
                ("○ WATCHERS SUSPENDED", Color32::from_rgb(255, 180, 50))
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

            // Card 1
            let rule1 = WatchFolderRule {
                path: "📁 /Volumes/Production/Camera_Ingest",
                preset: "Editing Proxies (ProRes 422 Proxy / 1080p)",
                filter: "*.mov, *.mp4, *.braw, *.mxf (> 100 MB)",
                target: "/Volumes/Production/Proxies/",
                post_action: "Tag Orange • Move to /Camera_Ingest/Processed/",
                stats: "142 Ingested • 0 Errors • Active (Polling every 5s)",
                is_active: true,
            };
            if render_watch_folder_card(ui, &rule1, state.selected_rule_idx == 0) {
                state.selected_rule_idx = 0;
            }

            ui.add_space(8.0);

            // Card 2
            let rule2 = WatchFolderRule {
                path: "📁 /Users/zachshallbetter/Movies/Master_Deliverables",
                preset: "Web Delivery (H.264 1080p) + EBU R128 Verification QC",
                filter: "*.mov (ProRes 422 HQ / Master)",
                target: "/Users/zachshallbetter/Deliverables/Web/",
                post_action: "Generate SHA-256 Receipt • System Notification",
                stats: "28 Ingested • 0 Errors • Active (Polling every 10s)",
                is_active: true,
            };
            if render_watch_folder_card(ui, &rule2, state.selected_rule_idx == 1) {
                state.selected_rule_idx = 1;
            }

            ui.add_space(8.0);

            // Card 3 (Scheduled Batch)
            let rule3 = WatchFolderRule {
                path: "📁 /Volumes/Archive/Nightly_Transcodes",
                preset: "Archive Mezzanine (ProRes 422 LT / 4K)",
                filter: "All Uncompressed Video Formats",
                target: "/Volumes/Archive/Mezzanine/",
                post_action: "Verify Checksum • Move Original to Cold Storage",
                stats: "Scheduled for 02:00 AM Daily • Next Run: in 4h 32m",
                is_active: false,
            };
            if render_watch_folder_card(ui, &rule3, state.selected_rule_idx == 2) {
                state.selected_rule_idx = 2;
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

                    let events = [
                        (
                            "Today 21:14:02",
                            "A001_C001.mov (1.2 GB)",
                            "Camera_Ingest -> Proxies",
                            "A001_C001_proxy.mp4",
                            "✓ Transcoding (Job #1)",
                        ),
                        (
                            "Today 21:11:45",
                            "Interview_B_Roll.mp4 (450 MB)",
                            "Camera_Ingest -> Proxies",
                            "Interview_B_Roll_web.mp4",
                            "✓ Transcoding (Job #2)",
                        ),
                        (
                            "Today 20:45:10",
                            "Episode_01_Master.mov (18.4 GB)",
                            "Master_Deliverables -> Web",
                            "Episode_01_Web.mp4",
                            "✓ Completed & QC Verified",
                        ),
                        (
                            "Today 19:30:22",
                            "Color_Bars_HD.mov (120 MB)",
                            "Camera_Ingest -> Proxies",
                            "Color_Bars_HD_proxy.mp4",
                            "✓ Completed (12.4s)",
                        ),
                    ];

                    for (time, src, rule, out, res) in events {
                        ui.label(
                            RichText::new(time)
                                .size(10.0)
                                .monospace()
                                .color(Color32::from_rgb(160, 175, 195)),
                        );
                        ui.label(RichText::new(src).size(10.5).strong());
                        ui.label(
                            RichText::new(rule)
                                .size(10.0)
                                .color(Color32::from_rgb(180, 215, 255)),
                        );
                        ui.label(
                            RichText::new(out)
                                .size(10.0)
                                .color(Color32::from_rgb(190, 200, 215)),
                        );
                        let res_color = if res.contains("Completed") {
                            Color32::from_rgb(80, 230, 120)
                        } else {
                            Color32::from_rgb(90, 190, 255)
                        };
                        ui.label(RichText::new(res).size(10.0).color(res_color).strong());
                        ui.end_row();
                    }
                });
        });
}

struct WatchFolderRule<'a> {
    path: &'a str,
    preset: &'a str,
    filter: &'a str,
    target: &'a str,
    post_action: &'a str,
    stats: &'a str,
    is_active: bool,
}

fn render_watch_folder_card(
    ui: &mut egui::Ui,
    rule: &WatchFolderRule<'_>,
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
        rule.path,
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
        rule.stats,
        FontId::proportional(9.5),
        Color32::GRAY,
    );

    resp.clicked()
}
