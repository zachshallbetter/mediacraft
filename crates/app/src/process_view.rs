//! Process Sidebar & Outcome Planner

use egui::{Color32, CornerRadius, RichText, Stroke, Vec2};
use mediacraft_catalog::{MediaAsset, OperationKind};
use mediacraft_protocol::EncodingPreset;

pub struct ProcessViewState {
    pub active_operation: OperationKind,
    pub selected_preset_idx: usize,
    pub target_bitrate_kbps: u32,
    pub resolution_scale: usize, // 0 = Same as source, 1 = 1080p, 2 = 720p
    pub faststart_moov: bool,
    pub audio_priming_compensation: bool,
    pub output_folder: String,
    pub file_naming: String,
}

impl Default for ProcessViewState {
    fn default() -> Self {
        Self {
            active_operation: OperationKind::Convert,
            selected_preset_idx: 0,
            target_bitrate_kbps: 45_000,
            resolution_scale: 0,
            faststart_moov: true,
            audio_priming_compensation: true,
            output_folder: "/Users/zachshallbetter/Downloads".to_string(),
            file_naming: "{name}_{preset}".to_string(),
        }
    }
}

pub fn render_process_panel(
    ui: &mut egui::Ui,
    state: &mut ProcessViewState,
    asset: Option<&MediaAsset>,
    presets: &[EncodingPreset],
    on_enqueue_job: &mut Option<(EncodingPreset, String, String)>,
) {
    ui.heading(
        RichText::new("Process & Delivery")
            .size(15.0)
            .color(Color32::from_rgb(230, 240, 255)),
    );
    ui.separator();

    // 1. Operation Selection Grid (Outcome-First)
    ui.label(RichText::new("Operation Outcome:").strong().size(11.5));
    ui.add_space(2.0);

    let operations = [
        (OperationKind::Convert, "⟳ Convert", "Transcode formats"),
        (OperationKind::Optimize, "⚡ Optimize", "Reduce size"),
        (OperationKind::Extract, "✂ Extract", "Audio / frames"),
        (OperationKind::Analyze, "📊 Analyze", "Quality metrics"),
        (OperationKind::Proxy, "❏ Proxy", "Editing proxies"),
        (
            OperationKind::ImageSequence,
            "🎞 Image Seq",
            "EXR / PNG plates",
        ),
    ];

    egui::Grid::new("operations_grid")
        .spacing(Vec2::new(6.0, 6.0))
        .show(ui, |ui| {
            for (i, (op, label, _desc)) in operations.iter().enumerate() {
                let is_selected = state.active_operation == *op;
                let btn = egui::Button::new(RichText::new(*label).size(11.0).strong().color(
                    if is_selected {
                        Color32::from_rgb(80, 200, 255)
                    } else {
                        Color32::from_rgb(200, 215, 235)
                    },
                ))
                .min_size(Vec2::new(100.0, 32.0))
                .fill(if is_selected {
                    Color32::from_rgb(32, 50, 75)
                } else {
                    Color32::from_rgb(24, 28, 36)
                })
                .stroke(if is_selected {
                    Stroke::new(1.5, Color32::from_rgb(80, 180, 255))
                } else {
                    Stroke::new(1.0, Color32::from_rgb(45, 52, 65))
                })
                .corner_radius(CornerRadius::same(4));

                if ui.add(btn).clicked() {
                    state.active_operation = *op;
                }

                if (i + 1) % 2 == 0 {
                    ui.end_row();
                }
            }
        });

    ui.add_space(8.0);
    ui.separator();

    // 2. Preset & Output Settings
    ui.label(
        RichText::new("Output Profile & Preset:")
            .strong()
            .size(11.5),
    );
    ui.add_space(2.0);

    let cur_preset_name = presets
        .get(state.selected_preset_idx)
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "Default Preset".to_string());

    egui::ComboBox::from_id_salt("process_preset_combo")
        .width(ui.available_width().max(180.0))
        .selected_text(RichText::new(cur_preset_name).strong())
        .show_ui(ui, |ui| {
            for (idx, p) in presets.iter().enumerate() {
                ui.selectable_value(&mut state.selected_preset_idx, idx, &p.name);
            }
        });

    ui.add_space(6.0);

    // Selected Preset Technical Details
    if let Some(preset) = presets.get(state.selected_preset_idx) {
        ui.group(|ui| {
            ui.label(
                RichText::new(format!(
                    "Container: {:?} • Video: {:?} • Audio: {:?}",
                    preset.container, preset.video_codec, preset.audio_codec
                ))
                .size(10.5)
                .color(Color32::from_rgb(180, 205, 235)),
            );
            ui.label(
                RichText::new(format!(
                    "ID: {} • Faststart: {} • Priming: {} samples",
                    preset.id, preset.faststart, preset.audio_priming_samples
                ))
                .size(10.0)
                .color(Color32::GRAY),
            );
        });
    }

    ui.add_space(6.0);

    // Resolution Scale Selector
    ui.horizontal(|ui| {
        ui.label(RichText::new("Resolution:").size(11.0));
        ui.selectable_value(&mut state.resolution_scale, 0, "Source");
        ui.selectable_value(&mut state.resolution_scale, 1, "1080p");
        ui.selectable_value(&mut state.resolution_scale, 2, "720p");
    });

    ui.add_space(4.0);

    // Target Bitrate Slider
    ui.label(RichText::new("Target Bitrate:").size(11.0));
    ui.add(egui::Slider::new(&mut state.target_bitrate_kbps, 2_000..=120_000).suffix(" kbps"));

    ui.horizontal(|ui| {
        ui.checkbox(&mut state.faststart_moov, "Faststart (moov before mdat)");
    });
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut state.audio_priming_compensation,
            "AAC 1024-sample Priming (elst)",
        );
    });

    ui.add_space(6.0);

    // Output Destination & File Naming
    ui.label(RichText::new("Destination:").strong().size(11.0));
    ui.text_edit_singleline(&mut state.output_folder);

    ui.add_space(4.0);
    ui.label(RichText::new("File Naming Pattern:").size(11.0));
    ui.text_edit_singleline(&mut state.file_naming);

    // File Size Estimation
    if let Some(asset) = asset {
        let duration = asset.duration_secs.unwrap_or(30.0);
        let est_mb =
            (duration as f64 * (state.target_bitrate_kbps as f64 * 1000.0) / 8.0) / 1_000_000.0;
        ui.add_space(6.0);
        ui.label(
            RichText::new(format!("Estimated Output Size: {:.1} MB", est_mb))
                .size(11.5)
                .color(Color32::from_rgb(90, 230, 130))
                .strong(),
        );
    }

    ui.add_space(10.0);

    // 3. Action Button: Add to Queue
    let add_btn = egui::Button::new(
        RichText::new("▶ Add to Queue")
            .size(13.0)
            .strong()
            .color(Color32::WHITE),
    )
    .min_size(Vec2::new(ui.available_width(), 36.0))
    .fill(Color32::from_rgb(20, 110, 235))
    .corner_radius(CornerRadius::same(5));

    if ui.add(add_btn).clicked() {
        let Some(asset) = asset else {
            return;
        };
        let mut preset = presets
            .get(state.selected_preset_idx)
            .cloned()
            .unwrap_or_default();
        preset.video_bitrate_kbps = Some(state.target_bitrate_kbps);
        preset.faststart = state.faststart_moov;
        preset.audio_priming_samples = if state.audio_priming_compensation {
            1024
        } else {
            0
        };

        let out_ext = match preset.container {
            mediacraft_protocol::ContainerFormat::Mp4 => "mp4",
            mediacraft_protocol::ContainerFormat::Mov => "mov",
            mediacraft_protocol::ContainerFormat::M4a => "m4a",
            _ => "mp4",
        };

        let out_name = format!(
            "{}_{}.{out_ext}",
            asset.id,
            state.active_operation.label().to_ascii_lowercase()
        );
        let out_path = format!("{}/{}", state.output_folder, out_name);

        *on_enqueue_job = Some((preset, asset.path.display().to_string(), out_path));
    }
}
