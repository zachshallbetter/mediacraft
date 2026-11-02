//! Media Asset Inspector, Preview Player, and Relationship Graph

use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Stroke, Vec2};
use mediacraft_catalog::{DerivativeGraph, MediaAsset, MediaKind};

use crate::media_preview::{MediaPreviewCache, paint_asset_preview};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewMode {
    Source,
    Output,
    ComparisonSplit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    Info,
    Streams,
    Metadata,
    Validation,
    Analysis,
    History,
    Related,
}

pub struct InspectorState {
    pub active_tab: InspectorTab,
    pub scrub_pos: f32, // 0.0 to 1.0
    pub is_playing: bool,
    pub preview_mode: PreviewMode,
    pub split_pos: f32, // 0.0 to 1.0
    pub sidecar_status_msg: Option<String>,
}

impl Default for InspectorState {
    fn default() -> Self {
        Self {
            active_tab: InspectorTab::Info,
            scrub_pos: 0.15,
            is_playing: false,
            preview_mode: PreviewMode::Source,
            split_pos: 0.50,
            sidecar_status_msg: None,
        }
    }
}

pub fn render_inspector_panel(
    ui: &mut egui::Ui,
    state: &mut InspectorState,
    asset: Option<&MediaAsset>,
    graph: &DerivativeGraph,
    previews: &mut MediaPreviewCache,
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

    // 1. Preview Mode Selector (Source | Output | ComparisonSplit)
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("MONITOR:")
                .size(10.0)
                .color(Color32::from_rgb(130, 145, 165))
                .strong(),
        );
        if ui
            .selectable_label(state.preview_mode == PreviewMode::Source, "Source Master")
            .clicked()
        {
            state.preview_mode = PreviewMode::Source;
        }
        if ui
            .selectable_label(state.preview_mode == PreviewMode::Output, "Output Proxy")
            .clicked()
        {
            state.preview_mode = PreviewMode::Output;
        }
        if ui
            .selectable_label(
                state.preview_mode == PreviewMode::ComparisonSplit,
                "A|B Split Wipe",
            )
            .clicked()
        {
            state.preview_mode = PreviewMode::ComparisonSplit;
        }

        if state.preview_mode == PreviewMode::ComparisonSplit {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("Wipe {:.0}%", state.split_pos * 100.0))
                        .size(10.0)
                        .color(Color32::from_rgb(80, 190, 255)),
                );
            });
        }
    });

    ui.add_space(2.0);

    // 2. Preview Monitor (16:9, responsive with max height constraint)
    let max_w_by_h = (ui.available_height() * 0.42 * (16.0 / 9.0)).max(260.0);
    let monitor_w = ui.available_width().min(max_w_by_h).clamp(260.0, 720.0);
    let monitor_h = monitor_w * (9.0 / 16.0);

    let (rect, resp) = if state.preview_mode == PreviewMode::ComparisonSplit {
        ui.allocate_exact_size(
            Vec2::new(monitor_w, monitor_h),
            egui::Sense::click_and_drag(),
        )
    } else {
        ui.allocate_exact_size(Vec2::new(monitor_w, monitor_h), egui::Sense::hover())
    };

    if state.preview_mode == PreviewMode::ComparisonSplit
        && (resp.dragged() || resp.clicked())
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let rel_x = ((pos.x - rect.min.x) / rect.width()).clamp(0.05, 0.95);
        state.split_pos = rel_x;
    }

    let painter = ui.painter_at(rect);

    // Draw monitor background & visual simulation
    painter.rect_filled(rect, CornerRadius::same(4), Color32::from_rgb(12, 14, 18));
    painter.rect_stroke(
        rect,
        CornerRadius::same(4),
        Stroke::new(1.0, Color32::from_rgb(45, 52, 65)),
        egui::StrokeKind::Inside,
    );

    // Frame graphic simulation based on asset & comparison wipe
    let inner_rect = rect.shrink(3.0);
    draw_player_frame(
        &painter,
        ui.ctx(),
        inner_rect,
        asset,
        state.scrub_pos,
        state.preview_mode,
        state.split_pos,
        previews,
    );

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

    // 3. Transport & Scrub Bar
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

            if asset.kind == MediaKind::Video {
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
            }
        });
    });

    // 2.5 Filmstrip preview track with interactive playhead, frame notches, and sprocket holes
    let strip_h = 24.0;
    let strip_w = ui.available_width().max(240.0);
    let (strip_rect, strip_resp) =
        ui.allocate_exact_size(Vec2::new(strip_w, strip_h), egui::Sense::click_and_drag());

    if (strip_resp.dragged() || strip_resp.clicked())
        && let Some(pos) = strip_resp.interact_pointer_pos()
    {
        state.scrub_pos = ((pos.x - strip_rect.min.x) / strip_rect.width()).clamp(0.0, 1.0);
    }

    let strip_painter = ui.painter_at(strip_rect);
    strip_painter.rect_filled(
        strip_rect,
        CornerRadius::same(3),
        Color32::from_rgb(14, 17, 22),
    );
    strip_painter.rect_stroke(
        strip_rect,
        CornerRadius::same(3),
        Stroke::new(1.0, Color32::from_rgb(38, 46, 58)),
        egui::StrokeKind::Inside,
    );

    // Filmstrip frame notches
    let frame_count = (strip_w / 38.0).floor().max(1.0) as usize;
    let frame_w = strip_w / (frame_count as f32);
    for i in 0..frame_count {
        let fx = strip_rect.min.x + (i as f32) * frame_w;
        let cell_rect = Rect::from_min_max(
            Pos2::new(fx + 1.0, strip_rect.min.y + 2.0),
            Pos2::new(fx + frame_w - 1.0, strip_rect.max.y - 2.0),
        );
        let shade = if i % 2 == 0 {
            Color32::from_rgb(24, 30, 40)
        } else {
            Color32::from_rgb(20, 25, 34)
        };
        strip_painter.rect_filled(cell_rect, CornerRadius::same(2), shade);

        // Sprocket hole tick marks at top & bottom edge
        strip_painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(fx + 2.0, strip_rect.min.y + 1.0),
                Vec2::new(3.0, 2.0),
            ),
            CornerRadius::same(0),
            Color32::from_rgb(8, 10, 14),
        );
        strip_painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(fx + 2.0, strip_rect.max.y - 3.0),
                Vec2::new(3.0, 2.0),
            ),
            CornerRadius::same(0),
            Color32::from_rgb(8, 10, 14),
        );
    }

    // Playhead indicator needle
    let playhead_x = strip_rect.min.x + state.scrub_pos * strip_rect.width();
    strip_painter.line_segment(
        [
            Pos2::new(playhead_x, strip_rect.min.y),
            Pos2::new(playhead_x, strip_rect.max.y),
        ],
        Stroke::new(2.0, Color32::from_rgb(255, 175, 45)),
    );
    // Playhead diamond at top
    strip_painter.circle_filled(
        Pos2::new(playhead_x, strip_rect.min.y + 3.0),
        3.5,
        Color32::from_rgb(255, 205, 75),
    );

    ui.add_space(4.0);

    // 3. Tab Switcher using craft_ui::tabs (accessible, squeeze-fitting tabs)
    let tabs = [
        (InspectorTab::Info, "Info", 44.0),
        (InspectorTab::Streams, "Streams", 60.0),
        (InspectorTab::Metadata, "Metadata", 66.0),
        (InspectorTab::Validation, "Validation", 72.0),
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
            InspectorTab::Metadata => render_metadata_tab(ui, state, asset),
            InspectorTab::Validation => render_validation_tab(ui, asset),
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
                        "{}x{} • {}\n{}, {}-bit",
                        v.width,
                        v.height,
                        format_frame_rate(v.fps),
                        v.codec,
                        v.bit_depth
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
                    "Dimensions: {}x{} • {}",
                    v.width,
                    v.height,
                    format_frame_rate(v.fps)
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

fn render_metadata_tab(ui: &mut egui::Ui, state: &mut InspectorState, asset: &MediaAsset) {
    ui.label(RichText::new("Indexed File Metadata:").strong().size(11.5));
    ui.add_space(4.0);

    let extension = asset
        .path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("Unknown");
    let path_display = asset.path.to_string_lossy().into_owned();
    let formatted_size = asset.formatted_size();
    for (label, value) in [
        ("File name", asset.filename.as_str()),
        ("File path", path_display.as_str()),
        ("File type", extension),
        ("File size", formatted_size.as_str()),
        ("Modified", asset.date_created.as_str()),
    ] {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(label)
                    .size(10.5)
                    .monospace()
                    .color(Color32::from_rgb(140, 170, 200)),
            );
            ui.label(RichText::new(value).size(10.5));
        });
    }

    ui.separator();
    ui.label(RichText::new("Asset Tags & Keywords:").size(11.0).strong());
    ui.horizontal_wrapped(|ui| {
        for t in &asset.tags {
            ui.label(
                RichText::new(format!("#{t}"))
                    .size(10.5)
                    .color(Color32::from_rgb(80, 190, 255)),
            );
        }
    });

    ui.add_space(8.0);
    ui.separator();
    ui.label(
        RichText::new("Portable Metadata Sidecars (ISO 16684-1 / JSON)")
            .strong()
            .size(11.5),
    );
    ui.add_space(4.0);

    let xmp_path = asset.path.with_extension("xmp");
    let has_xmp = xmp_path.is_file();

    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Sidecar Status:")
                .size(10.5)
                .color(Color32::GRAY),
        );
        if has_xmp {
            ui.label(
                RichText::new("XMP sidecar found")
                    .size(10.5)
                    .color(Color32::from_rgb(80, 230, 120))
                    .strong(),
            );
        } else {
            ui.label(
                RichText::new("No external sidecar found")
                    .size(10.5)
                    .color(Color32::from_rgb(180, 190, 205)),
            );
        }
    });

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui
            .button(
                RichText::new("⚡ Export XMP Sidecar (.xmp)")
                    .size(10.5)
                    .strong()
                    .color(Color32::from_rgb(80, 190, 255)),
            )
            .clicked()
        {
            let sidecar = mediacraft_catalog::sidecar::MediaSidecar {
                rating: Some(asset.rating),
                label: asset.tags.first().cloned(),
                tags: asset.tags.clone(),
                title: Some(asset.filename.clone()),
                description: asset
                    .camera_model
                    .as_ref()
                    .map(|camera| format!("Camera: {camera}")),
                creator: None,
            };
            match mediacraft_catalog::sidecar::write_sidecar(
                &asset.path,
                &sidecar,
                mediacraft_catalog::sidecar::SidecarFormat::Xmp,
            ) {
                Ok(p) => {
                    state.sidecar_status_msg =
                        Some(format!("Exported XMP sidecar to {}", p.display()));
                }
                Err(e) => {
                    state.sidecar_status_msg = Some(format!("Error: {e}"));
                }
            }
        }

        if ui
            .button(
                RichText::new("📄 Export JSON (.mcsidecar)")
                    .size(10.5)
                    .color(Color32::from_rgb(200, 220, 240)),
            )
            .clicked()
        {
            let sidecar = mediacraft_catalog::sidecar::MediaSidecar {
                rating: Some(asset.rating),
                label: asset.tags.first().cloned(),
                tags: asset.tags.clone(),
                title: Some(asset.filename.clone()),
                description: asset
                    .camera_model
                    .as_ref()
                    .map(|camera| format!("Camera: {camera}")),
                creator: None,
            };
            match mediacraft_catalog::sidecar::write_sidecar(
                &asset.path,
                &sidecar,
                mediacraft_catalog::sidecar::SidecarFormat::Json,
            ) {
                Ok(p) => {
                    state.sidecar_status_msg =
                        Some(format!("Exported JSON sidecar to {}", p.display()));
                }
                Err(e) => {
                    state.sidecar_status_msg = Some(format!("Error: {e}"));
                }
            }
        }
    });

    if let Some(msg) = &state.sidecar_status_msg {
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!("✓ {msg}"))
                .size(10.0)
                .color(Color32::from_rgb(80, 230, 120)),
        );
    }
}

fn render_analysis_tab(ui: &mut egui::Ui, _asset: &MediaAsset) {
    ui.label(RichText::new("Signal Analysis").strong().size(11.5));
    ui.add_space(6.0);
    ui.label(
        RichText::new("No analysis report is available for this file.")
            .size(11.0)
            .color(Color32::GRAY),
    );
}

fn render_history_tab(ui: &mut egui::Ui, _asset: &MediaAsset) {
    ui.label(RichText::new("Processing History").strong().size(11.5));
    ui.add_space(6.0);
    ui.label(
        RichText::new("No processing receipts are recorded for this file.")
            .size(11.0)
            .color(Color32::GRAY),
    );
}

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
        RichText::new(format!(
            "├── Generated Derivatives ({}):",
            derivatives.len()
        ))
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

fn render_validation_tab(ui: &mut egui::Ui, _asset: &MediaAsset) {
    ui.label(RichText::new("Delivery Validation").strong().size(11.5));
    ui.add_space(6.0);
    ui.label(
        RichText::new("No delivery validation report is available for this file.")
            .size(11.0)
            .color(Color32::GRAY),
    );
}

fn draw_overlay_badge(painter: &egui::Painter, rect: Rect, text: &str, is_source: bool) {
    let badge_rect = Rect::from_min_size(
        Pos2::new(rect.min.x + 8.0, rect.min.y + 8.0),
        Vec2::new(170.0, 18.0),
    );
    painter.rect_filled(
        badge_rect,
        CornerRadius::same(2),
        Color32::from_black_alpha(200),
    );
    let col = if is_source {
        Color32::from_rgb(100, 220, 255)
    } else {
        Color32::from_rgb(255, 190, 100)
    };
    painter.text(
        badge_rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(10.0),
        col,
    );
}

fn draw_raster_simulation(
    painter: &egui::Painter,
    ctx: &egui::Context,
    rect: Rect,
    asset: &MediaAsset,
    is_compressed: bool,
    seek_seconds: f32,
    previews: &mut MediaPreviewCache,
) {
    paint_asset_preview(previews, ctx, painter, rect, asset, seek_seconds);
    if is_compressed {
        painter.rect_filled(rect, CornerRadius::same(3), Color32::from_black_alpha(45));
        draw_overlay_badge(painter, rect, "NO OUTPUT PREVIEW", false);
    }
}

fn draw_player_frame(
    painter: &egui::Painter,
    ctx: &egui::Context,
    rect: Rect,
    asset: &MediaAsset,
    scrub: f32,
    mode: PreviewMode,
    split_pos: f32,
    previews: &mut MediaPreviewCache,
) {
    let seek_seconds = asset.duration_secs.unwrap_or_default() * scrub.clamp(0.0, 1.0);
    match mode {
        PreviewMode::Source => {
            draw_raster_simulation(painter, ctx, rect, asset, false, seek_seconds, previews);
            draw_overlay_badge(painter, rect, "SOURCE FILE", true);
        }
        PreviewMode::Output => {
            draw_raster_simulation(painter, ctx, rect, asset, true, seek_seconds, previews);
        }
        PreviewMode::ComparisonSplit => {
            let split_x =
                (rect.min.x + rect.width() * split_pos).clamp(rect.min.x + 12.0, rect.max.x - 12.0);
            let right_rect = Rect::from_min_max(Pos2::new(split_x, rect.min.y), rect.max);
            draw_raster_simulation(painter, ctx, rect, asset, false, seek_seconds, previews);
            painter.rect_filled(
                right_rect,
                CornerRadius::ZERO,
                Color32::from_black_alpha(70),
            );
            painter.line_segment(
                [
                    Pos2::new(split_x, rect.min.y),
                    Pos2::new(split_x, rect.max.y),
                ],
                Stroke::new(2.0, Color32::from_rgb(255, 190, 70)),
            );
            draw_overlay_badge(painter, right_rect, "NO OUTPUT PREVIEW", false);
        }
    }
}

fn format_frame_rate(fps: f32) -> String {
    if fps > 0.0 {
        format!("{fps:.3} fps")
    } else {
        "frame rate unknown".to_string()
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
