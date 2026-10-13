//! Asset Library Grid and Browser View (Digital Asset Management)

use egui::{Color32, CornerRadius, Pos2, Rect, RichText, Stroke, Vec2};
use mediacraft_catalog::{MediaAsset, MediaKind, SmartCollection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Grid,
    List,
    Details,
}

pub struct LibraryViewState {
    pub view_mode: ViewMode,
    pub card_size: f32, // Thumbnail size slider
    pub sort_by: SortOption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOption {
    DateAdded,
    Filename,
    FileSize,
    Duration,
}

impl Default for LibraryViewState {
    fn default() -> Self {
        Self {
            view_mode: ViewMode::Grid,
            card_size: 160.0,
            sort_by: SortOption::DateAdded,
        }
    }
}

pub fn render_library_view(
    ui: &mut egui::Ui,
    state: &mut LibraryViewState,
    active_collection: SmartCollection,
    search_query: &mut String,
    assets: &[&MediaAsset],
    selected_asset_id: &mut Option<String>,
) {
    // 1. Breadcrumb & Navigation Bar
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("📁 Macintosh HD › Users › zachshallbetter › Projects ›")
                .size(12.0)
                .color(Color32::from_rgb(140, 160, 185)),
        );
        ui.label(
            RichText::new("Media")
                .size(12.0)
                .strong()
                .color(Color32::from_rgb(220, 235, 255)),
        );

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // View Mode Toggles using craft_ui::buttons::IconButton
            let btn_style = crate::craft_theme::craft_icon_button_style();
            let btn_size = Vec2::new(22.0, 22.0);

            if craft_ui::buttons::IconButton::new("Details View", btn_size, &btn_style)
                .selected(state.view_mode == ViewMode::Details)
                .show(ui, |ui, rect, tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "ℹ",
                        egui::FontId::proportional(11.0),
                        tint,
                    );
                })
                .clicked()
            {
                state.view_mode = ViewMode::Details;
            }

            if craft_ui::buttons::IconButton::new("List View", btn_size, &btn_style)
                .selected(state.view_mode == ViewMode::List)
                .show(ui, |ui, rect, tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "☰",
                        egui::FontId::proportional(11.0),
                        tint,
                    );
                })
                .clicked()
            {
                state.view_mode = ViewMode::List;
            }

            if craft_ui::buttons::IconButton::new("Grid View", btn_size, &btn_style)
                .selected(state.view_mode == ViewMode::Grid)
                .show(ui, |ui, rect, tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "⊞",
                        egui::FontId::proportional(11.0),
                        tint,
                    );
                })
                .clicked()
            {
                state.view_mode = ViewMode::Grid;
            }

            ui.add_space(8.0);
            ui.label(RichText::new("Size:").size(11.0));
            ui.add(egui::Slider::new(&mut state.card_size, 110.0..=220.0).show_value(false));

            ui.add_space(8.0);
            egui::ComboBox::from_id_salt("sort_by_combo")
                .selected_text(match state.sort_by {
                    SortOption::DateAdded => "Sort: Date Added",
                    SortOption::Filename => "Sort: Filename",
                    SortOption::FileSize => "Sort: File Size",
                    SortOption::Duration => "Sort: Duration",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.sort_by, SortOption::DateAdded, "Date Added");
                    ui.selectable_value(&mut state.sort_by, SortOption::Filename, "Filename");
                    ui.selectable_value(&mut state.sort_by, SortOption::FileSize, "File Size");
                    ui.selectable_value(&mut state.sort_by, SortOption::Duration, "Duration");
                });
        });
    });

    ui.separator();

    // 2. Collection Header & Item Count
    ui.horizontal(|ui| {
        ui.heading(
            RichText::new(active_collection.label())
                .size(16.0)
                .color(Color32::from_rgb(240, 245, 255)),
        );
        ui.label(
            RichText::new(format!("{} items", assets.len()))
                .size(12.0)
                .color(Color32::from_rgb(140, 160, 185)),
        );

        if !search_query.is_empty() {
            ui.label(
                RichText::new(format!("matching \"{search_query}\""))
                    .size(12.0)
                    .italics()
                    .color(Color32::from_rgb(80, 190, 255)),
            );
            if ui.small_button("Clear").clicked() {
                search_query.clear();
            }
        }
    });

    ui.add_space(4.0);

    // 3. Asset Grid / List Scroll Area
    egui::ScrollArea::vertical()
        .id_salt("library_grid_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if assets.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(
                        RichText::new("No media assets found matching the active filters.")
                            .size(14.0)
                            .color(Color32::GRAY),
                    );
                });
                return;
            }

            match state.view_mode {
                ViewMode::Grid => {
                    render_grid_cards(ui, assets, state.card_size, selected_asset_id);
                }
                ViewMode::List => {
                    render_list_view(ui, assets, selected_asset_id);
                }
                ViewMode::Details => {
                    render_details_view(ui, assets, selected_asset_id);
                }
            }
        });
}

fn render_grid_cards(
    ui: &mut egui::Ui,
    assets: &[&MediaAsset],
    card_width: f32,
    selected_asset_id: &mut Option<String>,
) {
    let available_width = ui.available_width().max(300.0);
    let item_spacing = 10.0;
    let card_height = card_width * 0.92;
    let cols = ((available_width + item_spacing) / (card_width + item_spacing))
        .floor()
        .max(1.0) as usize;

    egui::Grid::new("asset_cards_grid")
        .spacing(Vec2::new(item_spacing, item_spacing))
        .show(ui, |ui| {
            for (idx, asset) in assets.iter().enumerate() {
                let is_selected = selected_asset_id.as_deref() == Some(&asset.id);

                ui.allocate_ui(Vec2::new(card_width, card_height), |ui| {
                    let (rect, resp) = ui.allocate_exact_size(
                        Vec2::new(card_width, card_height),
                        egui::Sense::click(),
                    );

                    if resp.clicked() {
                        *selected_asset_id = Some(asset.id.clone());
                    }

                    // Card Background
                    let bg_color = if is_selected {
                        Color32::from_rgb(26, 45, 75)
                    } else if resp.hovered() {
                        Color32::from_rgb(32, 38, 48)
                    } else {
                        Color32::from_rgb(22, 26, 33)
                    };

                    let border_stroke = if is_selected {
                        Stroke::new(2.0, Color32::from_rgb(80, 180, 255))
                    } else if resp.hovered() {
                        Stroke::new(1.0, Color32::from_rgb(70, 85, 105))
                    } else {
                        Stroke::new(1.0, Color32::from_rgb(40, 46, 56))
                    };

                    let painter = ui.painter_at(rect);
                    painter.rect_filled(rect, CornerRadius::same(5), bg_color);
                    painter.rect_stroke(
                        rect,
                        CornerRadius::same(5),
                        border_stroke,
                        egui::StrokeKind::Inside,
                    );

                    // 1. Thumbnail Area (top 62% of card)
                    let thumb_rect = Rect::from_min_max(
                        Pos2::new(rect.min.x + 3.0, rect.min.y + 3.0),
                        Pos2::new(rect.max.x - 3.0, rect.min.y + card_height * 0.62),
                    );

                    draw_rich_mock_thumbnail(&painter, thumb_rect, asset);

                    // Top Badges on Thumbnail
                    if asset.video.as_ref().is_some_and(|v| v.hdr) {
                        let badge_rect = Rect::from_min_size(
                            Pos2::new(thumb_rect.max.x - 30.0, thumb_rect.min.y + 4.0),
                            Vec2::new(26.0, 14.0),
                        );
                        painter.rect_filled(
                            badge_rect,
                            CornerRadius::same(2),
                            Color32::from_rgb(220, 150, 30),
                        );
                        painter.text(
                            badge_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "HDR",
                            egui::FontId::proportional(9.0),
                            Color32::BLACK,
                        );
                    }

                    // Bottom Badges on Thumbnail
                    let dur_rect = Rect::from_min_size(
                        Pos2::new(thumb_rect.min.x + 4.0, thumb_rect.max.y - 18.0),
                        Vec2::new(36.0, 14.0),
                    );
                    painter.rect_filled(
                        dur_rect,
                        CornerRadius::same(2),
                        Color32::from_black_alpha(180),
                    );
                    painter.text(
                        dur_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        asset.formatted_duration(),
                        egui::FontId::monospace(9.0),
                        Color32::WHITE,
                    );

                    // 2. Metadata Info Area (bottom of card)
                    let info_y = thumb_rect.max.y + 6.0;

                    // Filename
                    let filename_pos = Pos2::new(rect.min.x + 8.0, info_y);
                    painter.text(
                        filename_pos,
                        egui::Align2::LEFT_TOP,
                        truncate_filename(&asset.filename, (card_width / 8.5) as usize),
                        egui::FontId::proportional(11.5),
                        if is_selected {
                            Color32::from_rgb(240, 248, 255)
                        } else {
                            Color32::from_rgb(215, 225, 240)
                        },
                    );

                    // Codec Pill & Resolution
                    let pill_y = info_y + 16.0;
                    let codec_name = if let Some(ref v) = asset.video {
                        &v.codec
                    } else if let Some(ref a) = asset.audio {
                        &a.codec
                    } else {
                        asset.kind.label()
                    };

                    painter.text(
                        Pos2::new(rect.min.x + 8.0, pill_y),
                        egui::Align2::LEFT_TOP,
                        truncate_filename(codec_name, 16),
                        egui::FontId::proportional(10.0),
                        Color32::from_rgb(140, 165, 195),
                    );

                    // Resolution badge on the right
                    if let Some(ref v) = asset.video {
                        let res_text = if v.width >= 3840 {
                            "4K"
                        } else if v.width >= 1920 {
                            "1080p"
                        } else {
                            "HD"
                        };
                        painter.text(
                            Pos2::new(rect.max.x - 8.0, pill_y),
                            egui::Align2::RIGHT_TOP,
                            res_text,
                            egui::FontId::proportional(10.0),
                            Color32::from_rgb(90, 190, 255),
                        );
                    }
                });

                if (idx + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
}

fn render_list_view(
    ui: &mut egui::Ui,
    assets: &[&MediaAsset],
    selected_asset_id: &mut Option<String>,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Name").size(11.0).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new("Size").size(11.0).strong());
            ui.add_space(20.0);
            ui.label(RichText::new("Duration").size(11.0).strong());
            ui.add_space(20.0);
            ui.label(RichText::new("Codec").size(11.0).strong());
            ui.add_space(20.0);
            ui.label(RichText::new("Type").size(11.0).strong());
        });
    });
    ui.separator();

    for asset in assets {
        let is_selected = selected_asset_id.as_deref() == Some(&asset.id);

        let resp = ui.selectable_label(is_selected, RichText::new(&asset.filename).size(12.0));
        if resp.clicked() {
            *selected_asset_id = Some(asset.id.clone());
        }
    }
}

fn render_details_view(
    ui: &mut egui::Ui,
    assets: &[&MediaAsset],
    selected_asset_id: &mut Option<String>,
) {
    for asset in assets {
        let is_selected = selected_asset_id.as_deref() == Some(&asset.id);

        ui.group(|ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(is_selected, RichText::new(&asset.filename).strong())
                    .clicked()
                {
                    *selected_asset_id = Some(asset.id.clone());
                }
                ui.label(
                    RichText::new(asset.kind.label())
                        .size(11.0)
                        .color(Color32::GRAY),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(asset.formatted_size()).size(11.0).monospace());
                    ui.label(
                        RichText::new(asset.formatted_duration())
                            .size(11.0)
                            .monospace(),
                    );
                });
            });

            if let Some(ref v) = asset.video {
                ui.label(
                    RichText::new(format!(
                        "Video: {} • {}x{} @ {:.2} fps • {}",
                        v.codec, v.width, v.height, v.fps, v.color_space
                    ))
                    .size(10.5)
                    .color(Color32::from_rgb(160, 185, 215)),
                );
            }
        });
        ui.add_space(2.0);
    }
}

fn draw_rich_mock_thumbnail(painter: &egui::Painter, rect: Rect, asset: &MediaAsset) {
    let corner = CornerRadius::same(3);

    // Procedural visual gradients simulating real media frames
    match asset.kind {
        MediaKind::Video => {
            if asset.filename.contains("A001") || asset.filename.contains("Mountain") {
                // Alpine mountain sunset gradient
                painter.rect_filled(rect, corner, Color32::from_rgb(45, 75, 115));
                let peak1 = [
                    Pos2::new(rect.min.x, rect.max.y),
                    Pos2::new(
                        rect.min.x + rect.width() * 0.45,
                        rect.min.y + rect.height() * 0.35,
                    ),
                    Pos2::new(rect.max.x, rect.max.y),
                ];
                painter.add(egui::epaint::PathShape::convex_polygon(
                    peak1.to_vec(),
                    Color32::from_rgb(85, 110, 140),
                    Stroke::NONE,
                ));
            } else if asset.filename.contains("Color_Bars") {
                // SMPTE 75% Color Bars
                let bar_w = rect.width() / 7.0;
                let colors = [
                    Color32::from_rgb(190, 190, 190), // Gray
                    Color32::from_rgb(190, 190, 0),   // Yellow
                    Color32::from_rgb(0, 190, 190),   // Cyan
                    Color32::from_rgb(0, 190, 0),     // Green
                    Color32::from_rgb(190, 0, 190),   // Magenta
                    Color32::from_rgb(190, 0, 0),     // Red
                    Color32::from_rgb(0, 0, 190),     // Blue
                ];
                for (i, &col) in colors.iter().enumerate() {
                    let bx = rect.min.x + (i as f32) * bar_w;
                    let b_rect = Rect::from_min_max(
                        Pos2::new(bx, rect.min.y),
                        Pos2::new(bx + bar_w, rect.max.y),
                    );
                    painter.rect_filled(b_rect, CornerRadius::ZERO, col);
                }
            } else if asset.filename.contains("Motion") || asset.filename.contains("Graphics") {
                // Neon glow geometric pattern
                painter.rect_filled(rect, corner, Color32::from_rgb(15, 12, 35));
                let center = rect.center();
                painter.circle_stroke(
                    center,
                    rect.height() * 0.28,
                    Stroke::new(2.5, Color32::from_rgb(0, 230, 255)),
                );
                painter.circle_stroke(
                    center,
                    rect.height() * 0.18,
                    Stroke::new(2.0, Color32::from_rgb(255, 60, 180)),
                );
            } else if asset.filename.contains("Surf") {
                // Deep ocean cyan wave
                painter.rect_filled(rect, corner, Color32::from_rgb(10, 60, 95));
                painter.circle_filled(
                    Pos2::new(
                        rect.min.x + rect.width() * 0.7,
                        rect.min.y + rect.height() * 0.3,
                    ),
                    rect.height() * 0.35,
                    Color32::from_rgb(0, 180, 210),
                );
            } else if asset.filename.contains("Car") {
                // Red automotive paint studio tone
                painter.rect_filled(rect, corner, Color32::from_rgb(50, 15, 20));
                let wedge = [
                    Pos2::new(rect.min.x + rect.width() * 0.1, rect.max.y),
                    Pos2::new(
                        rect.min.x + rect.width() * 0.6,
                        rect.min.y + rect.height() * 0.4,
                    ),
                    Pos2::new(rect.max.x, rect.max.y),
                ];
                painter.add(egui::epaint::PathShape::convex_polygon(
                    wedge.to_vec(),
                    Color32::from_rgb(210, 30, 45),
                    Stroke::NONE,
                ));
            } else {
                // Cinematic neutral warm tone
                painter.rect_filled(rect, corner, Color32::from_rgb(35, 42, 50));
                painter.circle_filled(
                    rect.center(),
                    rect.height() * 0.25,
                    Color32::from_rgb(65, 80, 95),
                );
            }
        }
        MediaKind::Audio => {
            // Audio Waveform Visualizer simulation
            painter.rect_filled(rect, corner, Color32::from_rgb(18, 25, 32));
            let bars = 18;
            let bar_w = rect.width() / (bars as f32 * 1.5);
            for i in 0..bars {
                let h_factor = ((i as f32 * 0.7).sin().abs() * 0.6 + 0.2).clamp(0.1, 0.85);
                let bx = rect.min.x + (i as f32) * bar_w * 1.4 + 4.0;
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
        MediaKind::RawDng | MediaKind::Image => {
            // Photography tone
            painter.rect_filled(rect, corner, Color32::from_rgb(55, 40, 60));
            painter.circle_filled(
                rect.center(),
                rect.height() * 0.28,
                Color32::from_rgb(220, 120, 160),
            );
        }
        MediaKind::ImageSequence => {
            // EXR plate pattern
            painter.rect_filled(rect, corner, Color32::from_rgb(25, 45, 55));
            painter.rect_stroke(
                rect.shrink(8.0),
                CornerRadius::same(2),
                Stroke::new(1.5, Color32::from_rgb(100, 200, 220)),
                egui::StrokeKind::Inside,
            );
        }
        MediaKind::ProjectFile | MediaKind::Other => {
            painter.rect_filled(rect, corner, Color32::from_rgb(30, 34, 42));
        }
    }
}

fn truncate_filename(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let keep = max_len.saturating_sub(3);
        format!("{}...", &s[..keep.min(s.len())])
    }
}
