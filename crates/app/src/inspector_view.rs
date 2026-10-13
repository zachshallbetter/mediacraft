//! Media Asset Inspector, Preview Player, and Relationship Graph

use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Stroke, Vec2};
use mediacraft_catalog::{DerivativeGraph, MediaAsset, MediaKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    Info,
    Streams,
    Metadata,
    Analysis,
    History,
    Related,
}

pub struct InspectorState {
    pub active_tab: InspectorTab,
    pub scrub_pos: f32, // 0.0 to 1.0
    pub is_playing: bool,
    pub volume: f32,
}

impl Default for InspectorState {
    fn default() -> Self {
        Self {
            active_tab: InspectorTab::Info,
            scrub_pos: 0.15,
            is_playing: false,
            volume: 0.85,
        }
    }
}

pub fn render_inspector_panel(
    ui: &mut egui::Ui,
    state: &mut InspectorState,
    asset: Option<&MediaAsset>,
    graph: &DerivativeGraph,
) {
    let Some(asset) = asset else {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.label(
                RichText::new("No media asset selected.")
                    .size(13.0)
                    .color(Color32::GRAY),
            );
            ui.label(
                RichText::new("Select an item in the library to inspect.")
                    .size(11.0)
                    .color(Color32::DARK_GRAY),
            );
        });
        return;
    };

    // 1. Preview Monitor (16:9)
    let monitor_w = ui.available_width().max(260.0);
    let monitor_h = monitor_w * (9.0 / 16.0);

    let (rect, _resp) =
        ui.allocate_exact_size(Vec2::new(monitor_w, monitor_h), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // Draw monitor background & visual simulation
    painter.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(12, 14, 18));
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, Color32::from_rgb(45, 52, 65)),
        egui::StrokeKind::Inside,
    );

    // Frame graphic simulation based on asset
    let inner_rect = rect.shrink(3.0);
    draw_player_frame(&painter, inner_rect, asset, state.scrub_pos);

    // Timecode burn-in in corner
    let tc_rect = Rect::from_min_size(
        Pos2::new(rect.min.x + 8.0, rect.max.y - 24.0),
        Vec2::new(82.0, 16.0),
    );
    painter.rect_filled(
        tc_rect,
        CornerRadius::same(2),
        Color32::from_black_alpha(200),
    );
    painter.text(
        tc_rect.center(),
        egui::Align2::CENTER_CENTER,
        &asset.timecode_start,
        egui::FontId::monospace(10.5),
        Color32::from_rgb(80, 240, 140),
    );

    ui.add_space(4.0);

    // 2. Transport & Scrub Bar
    ui.horizontal(|ui| {
        let cur_timecode = format_timecode(asset.duration_secs.unwrap_or(0.0) * state.scrub_pos);
        let total_timecode = asset.formatted_duration();
        ui.label(
            RichText::new(format!("{cur_timecode} / {total_timecode}"))
                .size(10.5)
                .monospace(),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let btn_style = crate::craft_theme::craft_icon_button_style();
            let btn_size = Vec2::new(20.0, 20.0);

            let _ = craft_ui::buttons::IconButton::new("Fit View", btn_size, &btn_style).show(
                ui,
                |ui, rect, tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "⛶",
                        egui::FontId::proportional(10.0),
                        tint,
                    );
                },
            );
            ui.add_space(2.0);

            if craft_ui::buttons::IconButton::new("Mute", btn_size, &btn_style)
                .selected(state.volume == 0.0)
                .show(ui, |ui, rect, tint| {
                    let icon = if state.volume > 0.0 { "🔊" } else { "🔇" };
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        icon,
                        egui::FontId::proportional(10.0),
                        tint,
                    );
                })
                .clicked()
            {
                state.volume = if state.volume > 0.0 { 0.0 } else { 0.85 };
            }

            ui.add_space(3.0);
            if craft_ui::buttons::IconButton::new("Skip End", btn_size, &btn_style)
                .show(ui, |ui, rect, tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "⏭",
                        egui::FontId::proportional(10.0),
                        tint,
                    );
                })
                .clicked()
            {
                state.scrub_pos = 1.0;
            }

            if craft_ui::buttons::IconButton::new("Play", btn_size, &btn_style)
                .selected(state.is_playing)
                .show(ui, |ui, rect, tint| {
                    let icon = if state.is_playing { "⏸" } else { "▶" };
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        icon,
                        egui::FontId::proportional(10.0),
                        tint,
                    );
                })
                .clicked()
            {
                state.is_playing = !state.is_playing;
            }

            if craft_ui::buttons::IconButton::new("Skip Start", btn_size, &btn_style)
                .show(ui, |ui, rect, tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "⏮",
                        egui::FontId::proportional(10.0),
                        tint,
                    );
                })
                .clicked()
            {
                state.scrub_pos = 0.0;
            }

            ui.add_space(4.0);
            ui.add_sized(
                [42.0, 16.0],
                egui::Slider::new(&mut state.volume, 0.0..=1.0).show_value(false),
            );
        });
    });

    ui.add(egui::Slider::new(&mut state.scrub_pos, 0.0..=1.0).show_value(false));

    ui.add_space(4.0);

    // 3. Tab Switcher using craft_ui::tabs (accessible, squeeze-fitting tabs)
    let tabs = [
        (InspectorTab::Info, "Info", 44.0),
        (InspectorTab::Streams, "Streams", 60.0),
        (InspectorTab::Metadata, "Metadata", 66.0),
        (InspectorTab::Analysis, "Analysis", 62.0),
        (InspectorTab::History, "History", 56.0),
        (InspectorTab::Related, "Related", 58.0),
    ];

    let natural_widths: Vec<f32> = tabs.iter().map(|(_, _, w)| *w).collect();
    let selected_idx = tabs
        .iter()
        .position(|(t, _, _)| *t == state.active_tab)
        .unwrap_or(0);
    let available_w = ui.available_width().max(60.0);
    let tab_fit = craft_ui::tabs::fit(&natural_widths, selected_idx, available_w, 36.0, 24.0);

    ui.horizontal(|ui| {
        for (idx, width) in tab_fit.shown {
            if let Some((tab, label, _)) = tabs.get(idx) {
                let is_selected = state.active_tab == *tab;
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(width, 22.0), egui::Sense::hover());
                let tab_widget = craft_ui::tabs::Tab::new(
                    ui.id().with(("inspector_tab", idx)),
                    label,
                    is_selected,
                );
                let response = tab_widget.show_at(ui, rect, |ui, resp| {
                    let text_color = if is_selected {
                        Color32::from_rgb(90, 195, 255)
                    } else if resp.hovered() {
                        Color32::from_rgb(235, 245, 255)
                    } else {
                        Color32::from_rgb(155, 170, 190)
                    };
                    if is_selected {
                        let underline =
                            Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - 2.0), rect.max);
                        ui.painter().rect_filled(
                            underline,
                            CornerRadius::ZERO,
                            Color32::from_rgb(80, 190, 255),
                        );
                    }
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        *label,
                        egui::FontId::proportional(11.0),
                        text_color,
                    );
                });
                if response.clicked() {
                    state.active_tab = *tab;
                }
            }
        }
    });

    ui.separator();

    // 4. Tab Body Scroll Area
    egui::ScrollArea::vertical()
        .id_salt("inspector_tab_scroll")
        .show(ui, |ui| match state.active_tab {
            InspectorTab::Info => render_info_tab(ui, asset),
            InspectorTab::Streams => render_streams_tab(ui, asset),
            InspectorTab::Metadata => render_metadata_tab(ui, asset),
            InspectorTab::Analysis => render_analysis_tab(ui, asset),
            InspectorTab::History => render_history_tab(ui, asset),
            InspectorTab::Related => render_related_graph_tab(ui, asset, graph),
        });
}

fn render_info_tab(ui: &mut egui::Ui, asset: &MediaAsset) {
    egui::Grid::new("inspector_info_grid")
        .num_columns(2)
        .spacing(Vec2::new(12.0, 6.0))
        .show(ui, |ui| {
            ui.label(RichText::new("File Name:").size(11.0).color(Color32::GRAY));
            ui.label(RichText::new(&asset.filename).size(11.5).strong());
            ui.end_row();

            ui.label(RichText::new("File Path:").size(11.0).color(Color32::GRAY));
            ui.label(
                RichText::new(asset.path.display().to_string())
                    .size(10.5)
                    .color(Color32::from_rgb(170, 195, 220)),
            );
            ui.end_row();

            ui.label(RichText::new("Format:").size(11.0).color(Color32::GRAY));
            let fmt_str = asset
                .path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("Unknown")
                .to_uppercase();
            ui.label(RichText::new(fmt_str).size(11.0));
            ui.end_row();

            ui.label(RichText::new("File Size:").size(11.0).color(Color32::GRAY));
            ui.label(RichText::new(asset.formatted_size()).size(11.0).monospace());
            ui.end_row();

            ui.label(RichText::new("Duration:").size(11.0).color(Color32::GRAY));
            ui.label(
                RichText::new(asset.formatted_duration())
                    .size(11.0)
                    .monospace(),
            );
            ui.end_row();

            if let Some(ref v) = asset.video {
                ui.label(RichText::new("Video:").size(11.0).color(Color32::GRAY));
                ui.label(
                    RichText::new(format!(
                        "{}x{} (16:9), {:.3} fps\n{}, {}-bit",
                        v.width, v.height, v.fps, v.codec, v.bit_depth
                    ))
                    .size(10.5),
                );
                ui.end_row();

                ui.label(
                    RichText::new("Color Space:")
                        .size(11.0)
                        .color(Color32::GRAY),
                );
                ui.label(RichText::new(&v.color_space).size(11.0));
                ui.end_row();

                ui.label(RichText::new("HDR:").size(11.0).color(Color32::GRAY));
                ui.label(
                    RichText::new(if v.hdr {
                        "Yes (PQ / HLG)"
                    } else {
                        "None (SDR)"
                    })
                    .size(11.0),
                );
                ui.end_row();
            }

            if let Some(ref a) = asset.audio {
                ui.label(RichText::new("Audio:").size(11.0).color(Color32::GRAY));
                ui.label(
                    RichText::new(format!(
                        "{}, {} kHz, {} ch, {}-bit",
                        a.codec,
                        a.sample_rate / 1000,
                        a.channels,
                        a.bit_depth
                    ))
                    .size(10.5),
                );
                ui.end_row();
            }

            if let Some(ref cam) = asset.camera_model {
                ui.label(RichText::new("Camera:").size(11.0).color(Color32::GRAY));
                ui.label(RichText::new(cam).size(11.0));
                ui.end_row();
            }

            ui.label(
                RichText::new("Date Created:")
                    .size(11.0)
                    .color(Color32::GRAY),
            );
            ui.label(RichText::new(&asset.date_created).size(10.5));
            ui.end_row();

            if let Some(ref geo) = asset.geo_location {
                ui.label(RichText::new("Location:").size(11.0).color(Color32::GRAY));
                ui.label(RichText::new(geo).size(10.5));
                ui.end_row();
            }
        });

    ui.add_space(8.0);
    if ui.button("📂 Open in Finder").clicked() {}
}

fn render_streams_tab(ui: &mut egui::Ui, asset: &MediaAsset) {
    ui.label(
        RichText::new("Elementary Bitstream Tracks:")
            .strong()
            .size(11.5),
    );
    ui.add_space(4.0);

    if let Some(ref v) = asset.video {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("[Track 0: Video]")
                        .strong()
                        .color(Color32::from_rgb(80, 190, 255)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{}-bit", v.bit_depth)).size(10.0));
                });
            });
            ui.label(RichText::new(format!("Codec: {}", v.codec)).size(11.0));
            ui.label(
                RichText::new(format!(
                    "Dimensions: {}x{} @ {:.2} fps",
                    v.width, v.height, v.fps
                ))
                .size(11.0),
            );
            ui.label(RichText::new(format!("Color Matrix: {}", v.color_space)).size(11.0));
        });
        ui.add_space(4.0);
    }

    if let Some(ref a) = asset.audio {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("[Track 1: Audio]")
                        .strong()
                        .color(Color32::from_rgb(90, 230, 130)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{} kbps", a.bitrate_kbps)).size(10.0));
                });
            });
            ui.label(RichText::new(format!("Codec: {}", a.codec)).size(11.0));
            ui.label(
                RichText::new(format!(
                    "Rate: {} Hz • {} Channels",
                    a.sample_rate, a.channels
                ))
                .size(11.0),
            );
        });
        ui.add_space(4.0);
    }

    ui.group(|ui| {
        ui.label(
            RichText::new("[Track 2: Timecode]")
                .strong()
                .color(Color32::from_rgb(220, 180, 80)),
        );
        ui.label(
            RichText::new(format!("Start: {}", asset.timecode_start))
                .size(11.0)
                .monospace(),
        );
    });
}

fn render_metadata_tab(ui: &mut egui::Ui, asset: &MediaAsset) {
    ui.label(
        RichText::new("Embedded XMP / EXIF Attributes:")
            .strong()
            .size(11.5),
    );
    ui.add_space(4.0);

    for (k, v) in [
        ("xmp:CreatorTool", "ARRI Software v7.1"),
        ("tiff:ColorSpace", "1 (sRGB)"),
        ("xmpDM:audioSampleRate", "48000"),
        ("xmpDM:videoFrameRate", "23.976"),
        ("dc:format", "video/quicktime"),
    ] {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(k)
                    .size(10.5)
                    .monospace()
                    .color(Color32::from_rgb(140, 170, 200)),
            );
            ui.label(RichText::new(v).size(10.5));
        });
    }

    ui.separator();
    ui.label(RichText::new("Tags:").size(11.0).strong());
    ui.horizontal_wrapped(|ui| {
        for t in &asset.tags {
            ui.label(
                RichText::new(format!("#{t}"))
                    .size(10.5)
                    .color(Color32::from_rgb(80, 190, 255)),
            );
        }
    });
}

fn render_analysis_tab(ui: &mut egui::Ui, _asset: &MediaAsset) {
    ui.label(
        RichText::new("Perceptual Quality & Signal Analysis")
            .strong()
            .size(11.5),
    );
    ui.add_space(4.0);

    ui.group(|ui| {
        ui.label(RichText::new("VMAF Perceptual Score:").size(11.0));
        ui.horizontal(|ui| {
            ui.add(
                egui::ProgressBar::new(0.964)
                    .desired_width(160.0)
                    .fill(Color32::from_rgb(60, 220, 110)),
            );
            ui.label(
                RichText::new("96.4 / 100 (Broadcast Quality)")
                    .size(10.5)
                    .strong(),
            );
        });

        ui.add_space(4.0);
        ui.label(RichText::new("Peak Signal-to-Noise Ratio (PSNR):").size(11.0));
        ui.label(
            RichText::new("44.82 dB (Near Lossless)")
                .size(10.5)
                .color(Color32::from_rgb(90, 220, 130)),
        );

        ui.add_space(4.0);
        ui.label(RichText::new("Structural Similarity Index (SSIM):").size(11.0));
        ui.label(
            RichText::new("0.9924")
                .size(10.5)
                .color(Color32::from_rgb(90, 220, 130)),
        );
    });

    ui.add_space(4.0);
    ui.group(|ui| {
        ui.label(
            RichText::new("Audio Loudness (EBU R128):")
                .strong()
                .size(11.0),
        );
        ui.label(RichText::new("Integrated: -23.8 LUFS • True Peak: -1.2 dBTP").size(10.5));
    });
}

fn render_history_tab(ui: &mut egui::Ui, _asset: &MediaAsset) {
    ui.label(
        RichText::new("Asset Ingestion & Transformation Receipts")
            .strong()
            .size(11.5),
    );
    ui.add_space(4.0);

    let receipts = [
        (
            "2026-10-12 14:17:00",
            "Thumbnail extraction completed (JPEG 1080p, 420 KB)",
        ),
        (
            "2026-10-12 14:16:00",
            "Audio bounce completed (AAC 320k, 1024-sample priming elst)",
        ),
        (
            "2026-10-12 14:15:00",
            "Transcoded for Web (H.264 Faststart moov placed)",
        ),
        (
            "2026-10-12 14:10:00",
            "Generated editing proxy (H.264 1080p 5 Mbps)",
        ),
        (
            "2026-10-12 10:00:00",
            "Ingested from volume /Volumes/Production/Camera_Card_A",
        ),
    ];

    for (ts, desc) in receipts {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("✓")
                    .color(Color32::from_rgb(80, 230, 120))
                    .strong(),
            );
            ui.label(
                RichText::new(ts)
                    .size(10.0)
                    .monospace()
                    .color(Color32::GRAY),
            );
        });
        ui.label(RichText::new(desc).size(10.5));
        ui.add_space(2.0);
    }
}

/// The File Relationship Graph from mockup2.png
fn render_related_graph_tab(ui: &mut egui::Ui, asset: &MediaAsset, graph: &DerivativeGraph) {
    ui.label(
        RichText::new("File Relationship Graph (Derivatives & Proxies)")
            .strong()
            .size(12.0),
    );
    ui.label(
        RichText::new("Track originals, proxies, web deliveries, and extractions:")
            .size(10.5)
            .color(Color32::GRAY),
    );
    ui.add_space(6.0);

    let derivatives = graph.get_derivatives(&asset.id);

    // Root Original Node
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("🎞 Original Master")
                    .strong()
                    .color(Color32::from_rgb(255, 200, 80)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new("Source of Truth")
                        .size(10.0)
                        .color(Color32::from_rgb(180, 200, 220)),
                );
            });
        });
        ui.label(RichText::new(&asset.filename).size(12.0).strong());
        ui.label(
            RichText::new(format!(
                "{} • {}",
                asset.formatted_size(),
                asset.formatted_duration()
            ))
            .size(10.5)
            .color(Color32::GRAY),
        );
    });

    if derivatives.is_empty() {
        ui.add_space(8.0);
        ui.label(
            RichText::new("No derivatives generated yet for this asset.")
                .size(11.0)
                .italics()
                .color(Color32::GRAY),
        );
        return;
    }

    ui.add_space(4.0);
    ui.label(
        RichText::new("├── Generated Derivatives (4 active):")
            .size(11.0)
            .monospace()
            .color(Color32::from_rgb(120, 160, 200)),
    );
    ui.add_space(2.0);

    for deriv in derivatives {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                let badge_color = match deriv.operation {
                    mediacraft_catalog::OperationKind::Proxy => Color32::from_rgb(80, 180, 255),
                    mediacraft_catalog::OperationKind::Convert => Color32::from_rgb(90, 230, 130),
                    mediacraft_catalog::OperationKind::Extract => Color32::from_rgb(220, 140, 240),
                    _ => Color32::LIGHT_GRAY,
                };

                ui.label(
                    RichText::new(format!("[{}]", deriv.operation.label()))
                        .strong()
                        .color(badge_color),
                );
                ui.label(
                    RichText::new(
                        deriv
                            .path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("output"),
                    )
                    .size(11.5)
                    .strong(),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if deriv.verified {
                        ui.label(
                            RichText::new("✓ Verified")
                                .size(10.0)
                                .color(Color32::from_rgb(80, 230, 120)),
                        );
                    }
                });
            });

            ui.label(
                RichText::new(format!("Preset: {}", deriv.preset_name))
                    .size(10.5)
                    .color(Color32::from_rgb(170, 190, 215)),
            );
            let size_str = if deriv.size_bytes >= 1_000_000 {
                format!("{:.1} MB", deriv.size_bytes as f64 / 1_000_000.0)
            } else {
                format!("{} KB", deriv.size_bytes / 1000)
            };
            ui.label(
                RichText::new(format!(
                    "Generated: {} • {}",
                    deriv.date_generated, size_str
                ))
                .size(10.0)
                .color(Color32::GRAY),
            );
        });
        ui.add_space(2.0);
    }
}

fn draw_player_frame(painter: &egui::Painter, rect: Rect, asset: &MediaAsset, _scrub: f32) {
    let corner = CornerRadius::same(3);

    // Render visual player simulation matching asset kind
    match asset.kind {
        MediaKind::Video => {
            if asset.filename.contains("A001") {
                painter.rect_filled(rect, corner, Color32::from_rgb(35, 65, 105));
                let peak = [
                    Pos2::new(rect.min.x, rect.max.y),
                    Pos2::new(
                        rect.min.x + rect.width() * 0.48,
                        rect.min.y + rect.height() * 0.28,
                    ),
                    Pos2::new(rect.max.x, rect.max.y),
                ];
                painter.add(egui::epaint::PathShape::convex_polygon(
                    peak.to_vec(),
                    Color32::from_rgb(70, 95, 125),
                    Stroke::NONE,
                ));
            } else if asset.filename.contains("Color_Bars") {
                let bar_w = rect.width() / 7.0;
                let colors = [
                    Color32::from_rgb(190, 190, 190),
                    Color32::from_rgb(190, 190, 0),
                    Color32::from_rgb(0, 190, 190),
                    Color32::from_rgb(0, 190, 0),
                    Color32::from_rgb(190, 0, 190),
                    Color32::from_rgb(190, 0, 0),
                    Color32::from_rgb(0, 0, 190),
                ];
                for (i, &col) in colors.iter().enumerate() {
                    let bx = rect.min.x + (i as f32) * bar_w;
                    let b_rect = Rect::from_min_max(
                        Pos2::new(bx, rect.min.y),
                        Pos2::new(bx + bar_w, rect.max.y),
                    );
                    painter.rect_filled(b_rect, CornerRadius::ZERO, col);
                }
            } else {
                painter.rect_filled(rect, corner, Color32::from_rgb(25, 30, 38));
            }
        }
        MediaKind::Audio => {
            painter.rect_filled(rect, corner, Color32::from_rgb(16, 22, 28));
            let bars = 24;
            let bar_w = rect.width() / (bars as f32 * 1.4);
            for i in 0..bars {
                let h_factor = ((i as f32 * 0.5).sin().abs() * 0.7 + 0.15).clamp(0.1, 0.85);
                let bx = rect.min.x + (i as f32) * bar_w * 1.35 + 4.0;
                let bar_h = rect.height() * h_factor;
                let by = rect.center().y - bar_h * 0.5;
                let b_rect = Rect::from_min_size(Pos2::new(bx, by), Vec2::new(bar_w, bar_h));
                painter.rect_filled(
                    b_rect,
                    CornerRadius::same(1),
                    Color32::from_rgb(80, 220, 130),
                );
            }
        }
        _ => {
            painter.rect_filled(rect, corner, Color32::from_rgb(30, 35, 45));
        }
    }
}

fn format_timecode(secs: f32) -> String {
    let total_s = secs.round() as u32;
    let h = total_s / 3600;
    let m = (total_s % 3600) / 60;
    let s = total_s % 60;
    let f = ((secs.fract() * 24.0).round() as u32).min(23);
    format!("{h:02}:{m:02}:{s:02}:{f:02}")
}
