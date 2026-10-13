//! Presets Workspace (MC-10, MC-11, MC-12).
//!
//! Provides Preset Library browsing (MC-10), deep Preset Configuration Editor (MC-11),
//! and side-by-side Preset Comparison Matrix (MC-12) for verifying target contracts.

use egui::{Color32, CornerRadius, FontId, Pos2, RichText, Stroke, Vec2};
use mediacraft_protocol::{ContainerFormat, DeliveryContract, EncodingPreset};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetWorkspaceTab {
    Library,
    Editor,
    Comparison,
}

pub struct PresetViewState {
    pub active_tab: PresetWorkspaceTab,
    pub selected_preset_id: String,
    pub compare_a_id: String,
    pub compare_b_id: String,
    pub search_query: String,
    pub filter_container: Option<ContainerFormat>,
    pub editor_tab: usize,
    pub status_notification: Option<String>,
}

impl Default for PresetViewState {
    fn default() -> Self {
        Self {
            active_tab: PresetWorkspaceTab::Library,
            selected_preset_id: "match-source-hq".to_string(),
            compare_a_id: "prores-422hq-master".to_string(),
            compare_b_id: "web-delivery-1080p".to_string(),
            search_query: String::new(),
            filter_container: None,
            editor_tab: 0,
            status_notification: None,
        }
    }
}

pub fn render_preset_workspace(
    ui: &mut egui::Ui,
    state: &mut PresetViewState,
    presets: &mut Vec<EncodingPreset>,
) {
    let mut dismiss = false;
    if let Some(msg) = &state.status_notification {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("ℹ {msg}"))
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

    // 1. Top Sub-Header & Navigation Tabs
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("ENCODING PRESETS & CONTRACT SPECIFICATIONS")
                .size(11.5)
                .color(Color32::from_rgb(140, 160, 190))
                .strong(),
        );

        ui.add_space(16.0);

        let tabs = [
            ("📚 Preset Library (MC-10)", PresetWorkspaceTab::Library),
            ("⚙ Preset Editor (MC-11)", PresetWorkspaceTab::Editor),
            (
                "⚖ Preset Comparison (MC-12)",
                PresetWorkspaceTab::Comparison,
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
                .selectable_label(is_sel, RichText::new(label).color(text_color).size(11.0))
                .clicked()
            {
                state.active_tab = tab;
            }
        }
    });

    ui.separator();

    // 2. Main Workspace Body
    match state.active_tab {
        PresetWorkspaceTab::Library => render_preset_library(ui, state, presets),
        PresetWorkspaceTab::Editor => render_preset_editor(ui, state, presets),
        PresetWorkspaceTab::Comparison => render_preset_comparison(ui, state, presets),
    }
}

fn render_preset_library(
    ui: &mut egui::Ui,
    state: &mut PresetViewState,
    presets: &mut Vec<EncodingPreset>,
) {
    ui.columns(2, |columns| {
        // Left Column: Filterable List of Presets
        let left_ui = &mut columns[0];
        left_ui.horizontal(|ui| {
            ui.label(
                RichText::new("AVAILABLE PRESETS")
                    .size(10.5)
                    .color(Color32::from_rgb(130, 145, 165))
                    .strong(),
            );
            ui.add_space(6.0);
            ui.add(
                egui::TextEdit::singleline(&mut state.search_query)
                    .hint_text("Search presets...")
                    .desired_width(140.0),
            );
        });
        left_ui.add_space(4.0);

        left_ui.horizontal(|ui| {
            ui.label(RichText::new("Format:").size(10.0).color(Color32::GRAY));
            if ui
                .selectable_label(state.filter_container.is_none(), "All")
                .clicked()
            {
                state.filter_container = None;
            }
            if ui
                .selectable_label(state.filter_container == Some(ContainerFormat::Mp4), "MP4")
                .clicked()
            {
                state.filter_container = Some(ContainerFormat::Mp4);
            }
            if ui
                .selectable_label(state.filter_container == Some(ContainerFormat::Mov), "MOV")
                .clicked()
            {
                state.filter_container = Some(ContainerFormat::Mov);
            }
            if ui
                .selectable_label(state.filter_container == Some(ContainerFormat::M4a), "M4A")
                .clicked()
            {
                state.filter_container = Some(ContainerFormat::M4a);
            }
        });
        left_ui.add_space(4.0);

        let query = state.search_query.trim().to_lowercase();
        let selected_id = state.selected_preset_id.clone();

        egui::ScrollArea::vertical()
            .id_salt("preset_library_scroll")
            .max_height(left_ui.available_height() - 40.0)
            .show(left_ui, |ui| {
                for p in presets.iter() {
                    if matches!(state.filter_container, Some(cont) if p.container != cont) {
                        continue;
                    }
                    if !query.is_empty()
                        && !p.name.to_lowercase().contains(&query)
                        && !p.id.to_lowercase().contains(&query)
                    {
                        continue;
                    }

                    let is_sel = p.id == selected_id;
                    let card_w = (ui.available_width() - 8.0).max(240.0);
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(card_w, 62.0), egui::Sense::click());
                    let painter = ui.painter_at(rect);

                    let bg = if is_sel {
                        Color32::from_rgb(28, 36, 48)
                    } else {
                        Color32::from_rgb(20, 24, 30)
                    };
                    painter.rect_filled(rect, CornerRadius::same(5), bg);

                    let border_col = if is_sel {
                        Color32::from_rgb(80, 190, 255)
                    } else {
                        Color32::from_rgb(38, 46, 58)
                    };
                    painter.rect_stroke(
                        rect,
                        CornerRadius::same(5),
                        Stroke::new(if is_sel { 1.5 } else { 1.0 }, border_col),
                        egui::StrokeKind::Inside,
                    );

                    let tx = rect.min.x + 12.0;
                    painter.text(
                        Pos2::new(tx, rect.min.y + 8.0),
                        egui::Align2::LEFT_TOP,
                        &p.name,
                        FontId::proportional(11.5),
                        Color32::from_rgb(235, 242, 250),
                    );

                    let meta_desc = format!(
                        "{:?} • {:?} • {} kbps",
                        p.container,
                        p.video_codec,
                        p.video_bitrate_kbps.unwrap_or(0)
                    );
                    painter.text(
                        Pos2::new(tx, rect.min.y + 26.0),
                        egui::Align2::LEFT_TOP,
                        meta_desc,
                        FontId::proportional(10.0),
                        Color32::from_rgb(130, 180, 240),
                    );

                    let audio_desc = format!(
                        "{:?} • {} kbps",
                        p.audio_codec,
                        p.audio_bitrate_kbps.unwrap_or(0)
                    );
                    painter.text(
                        Pos2::new(tx, rect.min.y + 42.0),
                        egui::Align2::LEFT_TOP,
                        audio_desc,
                        FontId::proportional(9.0),
                        Color32::GRAY,
                    );

                    if resp.clicked() {
                        state.selected_preset_id = p.id.clone();
                    }
                    ui.add_space(4.0);
                }
            });

        // Right Column: Summary Card & Action Bar
        let right_ui = &mut columns[1];
        let maybe_preset = presets
            .iter()
            .find(|p| p.id == state.selected_preset_id)
            .cloned();

        if let Some(preset) = maybe_preset {
            right_ui.label(
                RichText::new("PRESET OVERVIEW & SPECIFICATION")
                    .size(10.5)
                    .color(Color32::from_rgb(130, 145, 165))
                    .strong(),
            );
            right_ui.add_space(8.0);

            right_ui.group(|ui| {
                ui.label(
                    RichText::new(&preset.name)
                        .size(13.0)
                        .strong()
                        .color(Color32::from_rgb(80, 190, 255)),
                );
                ui.label(
                    RichText::new(format!("Identifier: {}", preset.id))
                        .size(10.0)
                        .monospace()
                        .color(Color32::GRAY),
                );
                ui.add_space(4.0);
                ui.label(format!("• Container Format: {:?}", preset.container));
                ui.label(format!("• Video Codec: {:?}", preset.video_codec));
                if let (Some(w), Some(h)) = (preset.width, preset.height) {
                    ui.label(format!("• Target Dimensions: {w} × {h}"));
                } else {
                    ui.label("• Target Dimensions: Match Source");
                }
                ui.label(format!(
                    "• Video Bitrate: {} kbps",
                    preset.video_bitrate_kbps.unwrap_or(0)
                ));
                ui.label(format!("• Audio Codec: {:?}", preset.audio_codec));
                ui.label(format!(
                    "• Audio Bitrate: {} kbps",
                    preset.audio_bitrate_kbps.unwrap_or(0)
                ));
                ui.label(format!(
                    "• FastStart Web Streaming: {}",
                    if preset.faststart {
                        "Enabled (moov first)"
                    } else {
                        "Disabled"
                    }
                ));
                ui.label(format!(
                    "• Audio Priming Compensation: {} samples",
                    preset.audio_priming_samples
                ));
            });

            right_ui.add_space(8.0);
            right_ui.horizontal(|ui| {
                if ui
                    .button(RichText::new("✏ Edit Preset (MC-11)").size(11.0))
                    .clicked()
                {
                    state.active_tab = PresetWorkspaceTab::Editor;
                }
                if ui
                    .button(RichText::new("📋 Duplicate Preset").size(11.0))
                    .clicked()
                {
                    let mut dup = preset.clone();
                    dup.id = format!("{}_copy", preset.id);
                    dup.name = format!("{} (Copy)", preset.name);
                    presets.push(dup);
                    state.status_notification =
                        Some(format!("Duplicated preset '{}'", preset.name));
                }
                if ui
                    .button(RichText::new("⚖ Compare in Matrix (MC-12)").size(11.0))
                    .clicked()
                {
                    state.compare_a_id = preset.id.clone();
                    state.active_tab = PresetWorkspaceTab::Comparison;
                }
            });
        }
    });
}

fn render_preset_editor(
    ui: &mut egui::Ui,
    state: &mut PresetViewState,
    presets: &mut [EncodingPreset],
) {
    let preset = match presets
        .iter_mut()
        .find(|p| p.id == state.selected_preset_id)
    {
        Some(p) => p,
        None => {
            ui.label("Select a preset from the library to edit.");
            return;
        }
    };

    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Editing: {}", preset.name))
                .size(13.0)
                .strong()
                .color(Color32::from_rgb(80, 190, 255)),
        );
        ui.add_space(16.0);
        let tabs = ["Video", "Audio", "Multiplexer", "Delivery Contract"];
        for (idx, &tab_label) in tabs.iter().enumerate() {
            let is_sel = state.editor_tab == idx;
            if ui.selectable_label(is_sel, tab_label).clicked() {
                state.editor_tab = idx;
            }
        }
    });

    ui.separator();

    match state.editor_tab {
        0 => {
            ui.label(
                RichText::new("Video Encoding Parameters")
                    .strong()
                    .size(11.5),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Preset Name:");
                ui.add(egui::TextEdit::singleline(&mut preset.name).desired_width(260.0));
            });
            ui.add_space(4.0);
            let mut br = preset.video_bitrate_kbps.unwrap_or(20_000);
            if ui
                .add(
                    egui::Slider::new(&mut br, 1_000..=250_000)
                        .suffix(" kbps")
                        .text("Target Video Bitrate"),
                )
                .changed()
            {
                preset.video_bitrate_kbps = Some(br);
            }
        }
        1 => {
            ui.label(
                RichText::new("Audio Encoding Parameters")
                    .strong()
                    .size(11.5),
            );
            ui.add_space(4.0);
            let mut abr = preset.audio_bitrate_kbps.unwrap_or(320);
            if ui
                .add(
                    egui::Slider::new(&mut abr, 64..=640)
                        .suffix(" kbps")
                        .text("Audio Bitrate"),
                )
                .changed()
            {
                preset.audio_bitrate_kbps = Some(abr);
            }
            let mut prim = preset.audio_priming_samples;
            if ui
                .add(
                    egui::Slider::new(&mut prim, 0..=2048)
                        .suffix(" samples")
                        .text("Audio Priming Delay"),
                )
                .changed()
            {
                preset.audio_priming_samples = prim;
            }
        }
        2 => {
            ui.label(
                RichText::new("Multiplexer & Container Settings")
                    .strong()
                    .size(11.5),
            );
            ui.add_space(4.0);
            ui.checkbox(
                &mut preset.faststart,
                "FastStart MP4/M4A (relocate moov atom to start of file)",
            );
        }
        _ => {
            ui.label(
                RichText::new("Delivery Contract Verification Bounds")
                    .strong()
                    .size(11.5),
            );
            ui.add_space(4.0);
            let contract = preset
                .contract
                .get_or_insert_with(DeliveryContract::default);
            ui.checkbox(
                &mut contract.require_faststart,
                "Enforce FastStart Atom Placement",
            );
            ui.checkbox(
                &mut contract.require_audio_priming,
                "Enforce Audio Priming Sample Compensation",
            );
            let mut lufs = contract.target_lufs.unwrap_or(-24.0);
            if ui
                .add(
                    egui::Slider::new(&mut lufs, -30.0..=-10.0)
                        .suffix(" LUFS")
                        .text("Target Integrated Loudness"),
                )
                .changed()
            {
                contract.target_lufs = Some(lufs);
            }
        }
    }
}

fn render_preset_comparison(
    ui: &mut egui::Ui,
    state: &mut PresetViewState,
    presets: &[EncodingPreset],
) {
    ui.label(
        RichText::new("PRESET SPECIFICATION COMPARISON MATRIX (MC-12)")
            .size(10.5)
            .color(Color32::from_rgb(130, 145, 165))
            .strong(),
    );
    ui.add_space(6.0);

    // Pickers for preset A and preset B
    ui.horizontal(|ui| {
        ui.label("Preset A:");
        egui::ComboBox::from_id_salt("compare_preset_a_combo")
            .selected_text(
                presets
                    .iter()
                    .find(|p| p.id == state.compare_a_id)
                    .map(|p| p.name.as_str())
                    .unwrap_or("Select Preset A"),
            )
            .show_ui(ui, |ui| {
                for p in presets {
                    ui.selectable_value(&mut state.compare_a_id, p.id.clone(), &p.name);
                }
            });

        ui.add_space(16.0);
        ui.label("Preset B:");
        egui::ComboBox::from_id_salt("compare_preset_b_combo")
            .selected_text(
                presets
                    .iter()
                    .find(|p| p.id == state.compare_b_id)
                    .map(|p| p.name.as_str())
                    .unwrap_or("Select Preset B"),
            )
            .show_ui(ui, |ui| {
                for p in presets {
                    ui.selectable_value(&mut state.compare_b_id, p.id.clone(), &p.name);
                }
            });
    });

    ui.separator();

    let p_a = presets.iter().find(|p| p.id == state.compare_a_id);
    let p_b = presets.iter().find(|p| p.id == state.compare_b_id);

    match (p_a, p_b) {
        (Some(a), Some(b)) => {
            egui::Grid::new("preset_compare_grid")
                .striped(true)
                .num_columns(4)
                .spacing(Vec2::new(16.0, 8.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("Parameter").strong().size(11.0));
                    ui.label(
                        RichText::new(&a.name)
                            .strong()
                            .size(11.0)
                            .color(Color32::from_rgb(80, 190, 255)),
                    );
                    ui.label(
                        RichText::new(&b.name)
                            .strong()
                            .size(11.0)
                            .color(Color32::from_rgb(120, 220, 140)),
                    );
                    ui.label(RichText::new("Difference").strong().size(11.0));
                    ui.end_row();

                    let rows = [
                        (
                            "Container Format",
                            format!("{:?}", a.container),
                            format!("{:?}", b.container),
                            a.container != b.container,
                        ),
                        (
                            "Video Codec",
                            format!("{:?}", a.video_codec),
                            format!("{:?}", b.video_codec),
                            a.video_codec != b.video_codec,
                        ),
                        (
                            "Video Bitrate",
                            format!("{} kbps", a.video_bitrate_kbps.unwrap_or(0)),
                            format!("{} kbps", b.video_bitrate_kbps.unwrap_or(0)),
                            a.video_bitrate_kbps != b.video_bitrate_kbps,
                        ),
                        (
                            "Audio Codec",
                            format!("{:?}", a.audio_codec),
                            format!("{:?}", b.audio_codec),
                            a.audio_codec != b.audio_codec,
                        ),
                        (
                            "Audio Bitrate",
                            format!("{} kbps", a.audio_bitrate_kbps.unwrap_or(0)),
                            format!("{} kbps", b.audio_bitrate_kbps.unwrap_or(0)),
                            a.audio_bitrate_kbps != b.audio_bitrate_kbps,
                        ),
                        (
                            "FastStart Enabled",
                            format!("{}", a.faststart),
                            format!("{}", b.faststart),
                            a.faststart != b.faststart,
                        ),
                        (
                            "Audio Priming Delay",
                            format!("{} samples", a.audio_priming_samples),
                            format!("{} samples", b.audio_priming_samples),
                            a.audio_priming_samples != b.audio_priming_samples,
                        ),
                    ];

                    for (param, val_a, val_b, diff) in rows {
                        ui.label(RichText::new(param).size(10.5));
                        ui.label(RichText::new(val_a).size(10.5).monospace());
                        ui.label(RichText::new(val_b).size(10.5).monospace());
                        if diff {
                            ui.label(
                                RichText::new("⚡ Variance")
                                    .size(10.5)
                                    .strong()
                                    .color(Color32::from_rgb(240, 180, 60)),
                            );
                        } else {
                            ui.label(RichText::new("Identical").size(10.5).color(Color32::GRAY));
                        }
                        ui.end_row();
                    }
                });
        }
        _ => {
            ui.label("Select two presets above to compare their parameters.");
        }
    }
}
