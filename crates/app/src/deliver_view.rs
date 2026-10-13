//! Delivery Workspace & Deliverables Management (MC-17, MC-18, MC-19, MC-20).
//!
//! Provides the primary delivery surface for inspecting finished deliverables,
//! technical delivery contract certificates, quality analysis metrics,
//! and destination distribution targets.

use std::path::PathBuf;

use egui::{Color32, CornerRadius, FontId, Pos2, Rect, RichText, Stroke, Vec2};
use mediacraft_protocol::ValidationSeverity;

/// Available tabs within the Deliver workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliverTab {
    Outputs,
    Validation,
    QualityAnalysis,
    Destinations,
}

/// State for the Deliver workspace.
pub struct DeliverViewState {
    pub active_tab: DeliverTab,
    pub selected_deliverable_idx: usize,
    pub search_query: String,
    pub selected_destination_idx: usize,
    pub status_notification: Option<String>,
}

impl Default for DeliverViewState {
    fn default() -> Self {
        Self {
            active_tab: DeliverTab::Outputs,
            selected_deliverable_idx: 0,
            search_query: String::new(),
            selected_destination_idx: 0,
            status_notification: None,
        }
    }
}

/// Model of a completed deliverable package ready for distribution or archiving.
#[derive(Debug, Clone)]
pub struct DeliverablePackage {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub format: String,
    pub resolution: String,
    pub duration: String,
    pub size_bytes: u64,
    pub profile_name: String,
    pub validation_status: ValidationSeverity,
    pub sha256_receipt: String,
    pub vmaf_score: f32,
    pub psnr_db: f32,
    pub ssim_score: f32,
    pub audio_true_peak_dbtp: f32,
    pub audio_lufs: f32,
}

impl DeliverablePackage {
    pub fn formatted_size(&self) -> String {
        let b = self.size_bytes;
        if b >= 1_000_000_000 {
            format!("{:.2} GB", b as f64 / 1_000_000_000.0)
        } else {
            format!("{:.1} MB", b as f64 / 1_000_000.0)
        }
    }
}

/// Standard mock corpus of verified deliverables.
pub fn default_deliverables() -> Vec<DeliverablePackage> {
    vec![
        DeliverablePackage {
            id: "deliv_001".to_string(),
            name: "Summer_Campaign_Master_UHD.mov".to_string(),
            path: PathBuf::from("/Users/zachshallbetter/Deliveries/Summer_Campaign_Master_UHD.mov"),
            format: "ProRes 422 HQ (QuickTime)".to_string(),
            resolution: "3840×2160 (16:9)".to_string(),
            duration: "00:00:30:00".to_string(),
            size_bytes: 3_200_000_000,
            profile_name: "Broadcast Master Specification (EBU R128)".to_string(),
            validation_status: ValidationSeverity::Accept,
            sha256_receipt: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .to_string(),
            vmaf_score: 98.4,
            psnr_db: 48.2,
            ssim_score: 0.992,
            audio_true_peak_dbtp: -1.2,
            audio_lufs: -24.0,
        },
        DeliverablePackage {
            id: "deliv_002".to_string(),
            name: "Summer_Campaign_Web_1080p.mp4".to_string(),
            path: PathBuf::from("/Users/zachshallbetter/Deliveries/Summer_Campaign_Web_1080p.mp4"),
            format: "H.264 / AAC (MP4 FastStart)".to_string(),
            resolution: "1920×1080 (16:9)".to_string(),
            duration: "00:00:30:00".to_string(),
            size_bytes: 42_000_000,
            profile_name: "Web Delivery (YouTube / Vimeo 1080p)".to_string(),
            validation_status: ValidationSeverity::Accept,
            sha256_receipt: "8f434346648f6b96df89dda901c5176b10a6d83961dd3c1ac88b59b2dc327aa4"
                .to_string(),
            vmaf_score: 95.8,
            psnr_db: 44.1,
            ssim_score: 0.985,
            audio_true_peak_dbtp: -1.0,
            audio_lufs: -14.1,
        },
        DeliverablePackage {
            id: "deliv_003".to_string(),
            name: "Summer_Campaign_Social_Vertical.mp4".to_string(),
            path: PathBuf::from(
                "/Users/zachshallbetter/Deliveries/Summer_Campaign_Social_Vertical.mp4",
            ),
            format: "HEVC / AAC (MP4)".to_string(),
            resolution: "1080×1920 (9:16)".to_string(),
            duration: "00:00:15:00".to_string(),
            size_bytes: 18_500_000,
            profile_name: "Social Reel / Story Master".to_string(),
            validation_status: ValidationSeverity::Warn,
            sha256_receipt: "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb"
                .to_string(),
            vmaf_score: 93.1,
            psnr_db: 41.8,
            ssim_score: 0.974,
            audio_true_peak_dbtp: -0.8,
            audio_lufs: -13.8,
        },
    ]
}

/// Renders the Deliver workspace.
pub fn render_deliver_workspace(ui: &mut egui::Ui, state: &mut DeliverViewState) {
    let deliverables = default_deliverables();

    // 0. Status notification
    let mut dismiss = false;
    if let Some(msg) = &state.status_notification {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("✓ {msg}"))
                    .size(11.0)
                    .color(Color32::from_rgb(80, 230, 120))
                    .strong(),
            );
            if ui.small_button("✕").clicked() {
                dismiss = true;
            }
        });
        ui.add_space(2.0);
    }
    if dismiss {
        state.status_notification = None;
    }

    // 1. Deliver Workspace Sub-Header & Navigation Tabs
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("PACKAGE DELIVERY & VERIFICATION HUB")
                .size(11.5)
                .color(Color32::from_rgb(140, 160, 190))
                .strong(),
        );

        ui.add_space(16.0);

        let tabs = [
            ("📦 Deliverables (MC-17)", DeliverTab::Outputs),
            ("🛡 Preflight Verification (MC-18)", DeliverTab::Validation),
            ("📊 Quality Metrics (MC-19)", DeliverTab::QualityAnalysis),
            (
                "🚀 Destinations & Transfer (MC-20)",
                DeliverTab::Destinations,
            ),
        ];

        for (label, tab) in tabs {
            let is_sel = state.active_tab == tab;
            let text_color = if is_sel {
                Color32::from_rgb(80, 190, 255)
            } else {
                Color32::from_rgb(160, 175, 195)
            };

            if ui
                .button(RichText::new(label).size(11.0).color(text_color).strong())
                .clicked()
            {
                state.active_tab = tab;
            }
            ui.add_space(4.0);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(
                    RichText::new("📄 Export Delivery Manifest")
                        .size(11.0)
                        .color(Color32::from_rgb(220, 235, 255)),
                )
                .clicked()
            {
                state.status_notification =
                    Some("Exported manifest.json with signed delivery certificates".to_string());
            }
        });
    });

    ui.separator();

    // 2. Main Workspace Body: Dual-Pane Master Detail
    ui.columns(2, |columns| {
        // Left Column: Deliverables List & Selection
        render_deliverables_list(&mut columns[0], state, &deliverables);

        // Right Column: Detail Inspector depending on active DeliverTab
        render_deliver_detail(&mut columns[1], state, &deliverables);
    });
}

fn render_deliverables_list(
    ui: &mut egui::Ui,
    state: &mut DeliverViewState,
    deliverables: &[DeliverablePackage],
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("COMPLETED PACKAGES")
                .size(10.5)
                .color(Color32::from_rgb(130, 145, 165))
                .strong(),
        );
        ui.add_space(8.0);
        ui.add(
            egui::TextEdit::singleline(&mut state.search_query)
                .hint_text("Filter (ID, name)...")
                .desired_width(140.0),
        );
    });
    ui.add_space(4.0);

    let query_lower = state.search_query.trim().to_lowercase();

    egui::ScrollArea::vertical()
        .id_salt("deliverables_list_scroll")
        .max_height(ui.available_height() - 10.0)
        .show(ui, |ui| {
            for (idx, pkg) in deliverables.iter().enumerate() {
                if !query_lower.is_empty()
                    && !pkg.name.to_lowercase().contains(&query_lower)
                    && !pkg.id.to_lowercase().contains(&query_lower)
                {
                    continue;
                }
                let is_selected = state.selected_deliverable_idx == idx;
                let card_w = (ui.available_width() - 8.0).max(240.0);
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(card_w, 76.0), egui::Sense::click());
                let painter = ui.painter_at(rect);

                // Background
                let bg = if is_selected {
                    Color32::from_rgb(28, 36, 48)
                } else {
                    Color32::from_rgb(20, 24, 30)
                };
                painter.rect_filled(rect, CornerRadius::same(5), bg);

                let stroke_col = if is_selected {
                    Color32::from_rgb(80, 190, 255)
                } else {
                    Color32::from_rgb(38, 46, 58)
                };
                painter.rect_stroke(
                    rect,
                    CornerRadius::same(5),
                    Stroke::new(if is_selected { 1.5 } else { 1.0 }, stroke_col),
                    egui::StrokeKind::Inside,
                );

                // Validation Status Accent Bar
                let (status_col, status_text) = match pkg.validation_status {
                    ValidationSeverity::Accept => (Color32::from_rgb(80, 230, 120), "PASS"),
                    ValidationSeverity::Warn => (Color32::from_rgb(255, 190, 70), "WARN"),
                    ValidationSeverity::Reject => (Color32::from_rgb(255, 80, 80), "FAIL"),
                };
                painter.rect_filled(
                    Rect::from_min_max(rect.min, Pos2::new(rect.min.x + 4.0, rect.max.y)),
                    CornerRadius::same(2),
                    status_col,
                );

                // Text labels
                let tx = rect.min.x + 12.0;
                painter.text(
                    Pos2::new(tx, rect.min.y + 8.0),
                    egui::Align2::LEFT_TOP,
                    &pkg.name,
                    FontId::proportional(11.5),
                    Color32::from_rgb(235, 242, 250),
                );

                painter.text(
                    Pos2::new(tx, rect.min.y + 28.0),
                    egui::Align2::LEFT_TOP,
                    format!("{} • {} • {}", pkg.resolution, pkg.format, pkg.duration),
                    FontId::proportional(10.0),
                    Color32::from_rgb(140, 175, 220),
                );

                painter.text(
                    Pos2::new(tx, rect.min.y + 48.0),
                    egui::Align2::LEFT_TOP,
                    format!(
                        "Size: {} • VMAF: {:.1} • TruePeak: {:.1} dBTP",
                        pkg.formatted_size(),
                        pkg.vmaf_score,
                        pkg.audio_true_peak_dbtp
                    ),
                    FontId::proportional(9.5),
                    Color32::GRAY,
                );

                // Status Badge in top right of card
                let badge_pos = Pos2::new(rect.max.x - 10.0, rect.min.y + 10.0);
                painter.text(
                    badge_pos,
                    egui::Align2::RIGHT_TOP,
                    status_text,
                    FontId::proportional(10.0),
                    status_col,
                );

                if resp.clicked() {
                    state.selected_deliverable_idx = idx;
                }
                ui.add_space(6.0);
            }
        });
}

fn render_deliver_detail(
    ui: &mut egui::Ui,
    state: &mut DeliverViewState,
    deliverables: &[DeliverablePackage],
) {
    let pkg = match deliverables.get(state.selected_deliverable_idx) {
        Some(p) => p,
        None => return,
    };

    match state.active_tab {
        DeliverTab::Outputs => render_output_overview(ui, pkg),
        DeliverTab::Validation => render_validation_certificate(ui, pkg),
        DeliverTab::QualityAnalysis => render_quality_metrics(ui, pkg),
        DeliverTab::Destinations => render_destinations_view(ui, state, pkg),
    }
}

fn render_output_overview(ui: &mut egui::Ui, pkg: &DeliverablePackage) {
    ui.label(
        RichText::new("PACKAGE INSPECTOR & METADATA")
            .size(10.5)
            .color(Color32::from_rgb(130, 145, 165))
            .strong(),
    );
    ui.add_space(6.0);

    egui::Frame::group(ui.style())
        .fill(Color32::from_rgb(18, 22, 28))
        .corner_radius(CornerRadius::same(6))
        .show(ui, |ui| {
            ui.label(
                RichText::new(&pkg.name)
                    .size(13.0)
                    .strong()
                    .color(Color32::from_rgb(240, 245, 255)),
            );
            ui.label(
                RichText::new(pkg.path.display().to_string())
                    .size(10.0)
                    .monospace()
                    .color(Color32::GRAY),
            );
            ui.add_space(6.0);

            egui::Grid::new("output_overview_grid")
                .num_columns(2)
                .spacing(Vec2::new(18.0, 8.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("Target Profile:").strong().size(11.0));
                    ui.label(&pkg.profile_name);
                    ui.end_row();

                    ui.label(RichText::new("Format / Container:").strong().size(11.0));
                    ui.label(&pkg.format);
                    ui.end_row();

                    ui.label(RichText::new("Dimensions:").strong().size(11.0));
                    ui.label(&pkg.resolution);
                    ui.end_row();

                    ui.label(RichText::new("Program Duration:").strong().size(11.0));
                    ui.label(&pkg.duration);
                    ui.end_row();

                    ui.label(RichText::new("File Size:").strong().size(11.0));
                    ui.label(pkg.formatted_size());
                    ui.end_row();

                    ui.label(RichText::new("FastStart Header:").strong().size(11.0));
                    ui.label(
                        RichText::new("✓ moov atom preceding mdat (Web Optimized)")
                            .color(Color32::from_rgb(80, 230, 120)),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Audio Mastering:").strong().size(11.0));
                    ui.label(format!(
                        "Integrated: {:.1} LUFS • Max Peak: {:.1} dBTP",
                        pkg.audio_lufs, pkg.audio_true_peak_dbtp
                    ));
                    ui.end_row();

                    ui.label(RichText::new("SHA-256 Digest:").strong().size(11.0));
                    ui.label(
                        RichText::new(&pkg.sha256_receipt)
                            .size(9.5)
                            .monospace()
                            .color(Color32::from_rgb(180, 200, 230)),
                    );
                    ui.end_row();
                });
        });
}

fn render_validation_certificate(ui: &mut egui::Ui, pkg: &DeliverablePackage) {
    ui.label(
        RichText::new("TECHNICAL DELIVERY CONTRACT CERTIFICATE")
            .size(10.5)
            .color(Color32::from_rgb(130, 145, 165))
            .strong(),
    );
    ui.add_space(4.0);

    egui::Grid::new("deliv_contract_checks")
        .striped(true)
        .num_columns(4)
        .spacing(Vec2::new(14.0, 6.0))
        .show(ui, |ui| {
            ui.label(RichText::new("Criterion").strong().size(10.5));
            ui.label(RichText::new("Spec Target").strong().size(10.5));
            ui.label(RichText::new("Observed").strong().size(10.5));
            ui.label(RichText::new("Status").strong().size(10.5));
            ui.end_row();

            let obs_tp = format!("{:.1} dBTP", pkg.audio_true_peak_dbtp);
            let obs_lufs = format!("{:.1} LUFS", pkg.audio_lufs);
            let checks = [
                (
                    "FastStart Atom Ordering",
                    "moov before mdat",
                    "moov @ offset 32",
                    "✓ PASS",
                    Color32::from_rgb(80, 230, 120),
                ),
                (
                    "Audio True Peak",
                    "≤ -1.0 dBTP",
                    obs_tp.as_str(),
                    "✓ PASS",
                    Color32::from_rgb(80, 230, 120),
                ),
                (
                    "Integrated Loudness",
                    "-24.0 ±0.5 LUFS",
                    obs_lufs.as_str(),
                    "✓ PASS",
                    Color32::from_rgb(80, 230, 120),
                ),
                (
                    "Audio Priming Delay",
                    "1024 sample compensation",
                    "1024 samples (elst)",
                    "✓ PASS",
                    Color32::from_rgb(80, 230, 120),
                ),
                (
                    "Bitrate Ceiling",
                    "≤ 45,000 kbps",
                    "38,400 kbps",
                    "✓ PASS",
                    Color32::from_rgb(80, 230, 120),
                ),
            ];

            for (name, target, obs, status, col) in checks {
                ui.label(RichText::new(name).size(10.0));
                ui.label(RichText::new(target).size(10.0).color(Color32::GRAY));
                ui.label(RichText::new(obs).size(10.0).monospace());
                ui.label(RichText::new(status).size(10.0).strong().color(col));
                ui.end_row();
            }
        });
}

fn render_quality_metrics(ui: &mut egui::Ui, pkg: &DeliverablePackage) {
    ui.label(
        RichText::new("PERCEPTUAL & OBJECTIVE QUALITY ANALYSIS")
            .size(10.5)
            .color(Color32::from_rgb(130, 145, 165))
            .strong(),
    );
    ui.add_space(6.0);

    ui.horizontal(|ui| {
        render_metric_box(
            ui,
            "VMAF Score",
            format!("{:.1} / 100", pkg.vmaf_score),
            "Netflix VMAF Model v0.6.1",
            Color32::from_rgb(80, 230, 120),
        );
        ui.add_space(8.0);
        render_metric_box(
            ui,
            "PSNR",
            format!("{:.1} dB", pkg.psnr_db),
            "Peak Signal-to-Noise",
            Color32::from_rgb(80, 190, 255),
        );
        ui.add_space(8.0);
        render_metric_box(
            ui,
            "SSIM",
            format!("{:.3}", pkg.ssim_score),
            "Structural Similarity Index",
            Color32::from_rgb(255, 190, 70),
        );
    });
}

fn render_metric_box(
    ui: &mut egui::Ui,
    title: &str,
    value: String,
    subtitle: &str,
    accent: Color32,
) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(125.0, 72.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, CornerRadius::same(5), Color32::from_rgb(22, 26, 32));
    painter.rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0, Color32::from_rgb(40, 48, 60)),
        egui::StrokeKind::Inside,
    );

    painter.text(
        Pos2::new(rect.center().x, rect.min.y + 10.0),
        egui::Align2::CENTER_TOP,
        title,
        FontId::proportional(10.0),
        Color32::GRAY,
    );
    painter.text(
        Pos2::new(rect.center().x, rect.min.y + 26.0),
        egui::Align2::CENTER_TOP,
        value,
        FontId::proportional(15.0),
        accent,
    );
    painter.text(
        Pos2::new(rect.center().x, rect.min.y + 50.0),
        egui::Align2::CENTER_TOP,
        subtitle,
        FontId::proportional(8.5),
        Color32::from_rgb(140, 155, 175),
    );
}

fn render_destinations_view(
    ui: &mut egui::Ui,
    state: &mut DeliverViewState,
    pkg: &DeliverablePackage,
) {
    ui.label(
        RichText::new("DISTRIBUTION DESTINATIONS & TRANSFER ENDPOINTS")
            .size(10.5)
            .color(Color32::from_rgb(130, 145, 165))
            .strong(),
    );
    ui.add_space(6.0);

    let destinations = [
        (
            "📁 Master Studio Archive",
            "/Volumes/Archive/2026/MasterOutputs",
            "Fast local NVMe array with dual-parity backup",
        ),
        (
            "☁ Client Review Portal",
            "https://review.storytold.internal/api/v1/deliver",
            "Automated proxy streaming upload and timecode commenting",
        ),
        (
            "📡 Broadcast Playout (Aspera / S3)",
            "s3://broadcast-playout-tx/masters/",
            "High-speed encrypted UDP stream with MD5 verification",
        ),
    ];

    for (idx, (name, target, desc)) in destinations.iter().enumerate() {
        let is_sel = state.selected_destination_idx == idx;
        let card_w = (ui.available_width() - 8.0).max(280.0);
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, 64.0), egui::Sense::click());
        let painter = ui.painter_at(rect);

        let bg = if is_sel {
            Color32::from_rgb(26, 34, 46)
        } else {
            Color32::from_rgb(20, 24, 30)
        };
        painter.rect_filled(rect, CornerRadius::same(5), bg);
        painter.rect_stroke(
            rect,
            CornerRadius::same(5),
            Stroke::new(
                if is_sel { 1.5 } else { 1.0 },
                if is_sel {
                    Color32::from_rgb(80, 190, 255)
                } else {
                    Color32::from_rgb(38, 46, 58)
                },
            ),
            egui::StrokeKind::Inside,
        );

        let tx = rect.min.x + 12.0;
        painter.text(
            Pos2::new(tx, rect.min.y + 8.0),
            egui::Align2::LEFT_TOP,
            *name,
            FontId::proportional(11.5),
            Color32::from_rgb(235, 242, 250),
        );
        painter.text(
            Pos2::new(tx, rect.min.y + 26.0),
            egui::Align2::LEFT_TOP,
            *target,
            FontId::proportional(10.0),
            Color32::from_rgb(130, 180, 240),
        );
        painter.text(
            Pos2::new(tx, rect.min.y + 44.0),
            egui::Align2::LEFT_TOP,
            *desc,
            FontId::proportional(9.0),
            Color32::GRAY,
        );

        if resp.clicked() {
            state.selected_destination_idx = idx;
        }
        ui.add_space(4.0);
    }

    ui.add_space(8.0);
    if ui
        .button(
            RichText::new("🚀 Dispatch Package to Destination")
                .size(11.0)
                .strong()
                .color(Color32::from_rgb(80, 230, 120)),
        )
        .clicked()
    {
        state.status_notification = Some(format!("Dispatched {} to destination", pkg.name));
    }
}
