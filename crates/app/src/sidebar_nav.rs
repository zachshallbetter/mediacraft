//! Left Navigation Rail (Sources, Locations, Smart Collections, Facets)

use egui::{Color32, CornerRadius, Rect, RichText, Stroke, Vec2};
use mediacraft_catalog::{MediaKind, SmartCollection};
use std::collections::HashMap;

pub struct SidebarNavState {
    pub selected_collection: SmartCollection,
    pub selected_custom_collection_id: Option<String>,
    pub selected_custom_label: Option<String>,
    pub selected_kind_filter: Option<MediaKind>,
    pub selected_location: String,
}

impl Default for SidebarNavState {
    fn default() -> Self {
        Self {
            selected_collection: SmartCollection::AllMedia,
            selected_custom_collection_id: None,
            selected_custom_label: None,
            selected_kind_filter: None,
            selected_location: "Projects".to_string(),
        }
    }
}

pub fn render_sidebar_nav(
    ui: &mut egui::Ui,
    state: &mut SidebarNavState,
    kind_counts: &HashMap<MediaKind, usize>,
    total_assets: usize,
) {
    egui::ScrollArea::vertical()
        .id_salt("sidebar_nav_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // 1. Sources / Favorites
            ui.label(
                RichText::new("SOURCES")
                    .size(10.5)
                    .color(Color32::from_rgb(130, 145, 165))
                    .strong(),
            );
            ui.add_space(2.0);

            render_nav_item(
                ui,
                "🗃 All Media",
                state.selected_custom_collection_id.is_none()
                    && state.selected_collection == SmartCollection::AllMedia
                    && state.selected_kind_filter.is_none(),
                Some(total_assets),
                || {
                    state.selected_collection = SmartCollection::AllMedia;
                    state.selected_custom_collection_id = None;
                    state.selected_custom_label = None;
                    state.selected_kind_filter = None;
                },
            );

            render_nav_item(ui, "⭐ Favorites", false, None, || {});
            render_nav_item(ui, "🕒 Recents", false, None, || {});
            render_nav_item(ui, "🗑 Trash", false, None, || {});

            ui.add_space(8.0);
            ui.separator();

            // 2. Locations
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("LOCATIONS")
                        .size(10.5)
                        .color(Color32::from_rgb(130, 145, 165))
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("＋").clicked() {}
                });
            });
            ui.add_space(2.0);

            for loc in [
                ("💻 Macintosh HD", "Macintosh HD"),
                ("📁 Projects", "Projects"),
                ("📥 Downloads", "Downloads"),
                ("📷 Photography", "Photography"),
                ("🎬 Video", "Video"),
                ("🎵 Audio", "Audio"),
                ("🎨 Graphics", "Graphics"),
                ("🌐 Shared", "Shared"),
                ("🖧 Network", "Network"),
            ] {
                let is_selected = state.selected_location == loc.1;
                render_nav_item(ui, loc.0, is_selected, None, || {
                    state.selected_location = loc.1.to_string();
                });
            }

            ui.add_space(8.0);
            ui.separator();

            // 3. Dynamic Smart Collections
            ui.label(
                RichText::new("SMART COLLECTIONS")
                    .size(10.5)
                    .color(Color32::from_rgb(130, 145, 165))
                    .strong(),
            );
            ui.add_space(2.0);

            let collections = [
                (
                    SmartCollection::ClientDeliverables,
                    "📦 Client Deliverables",
                    None,
                ),
                (SmartCollection::EditingProxies, "❏ Editing Proxies", None),
                (
                    SmartCollection::NeedsTranscode,
                    "⚡ Needs Transcode",
                    Some(5),
                ),
                (SmartCollection::FourKOriginals, "🎞 4K Originals", Some(8)),
                (
                    SmartCollection::AudioForReview,
                    "🎙 Audio for Review",
                    Some(1),
                ),
                (
                    SmartCollection::ImageSequences,
                    "🎞 Image Sequences",
                    Some(1),
                ),
            ];

            for (col, label, badge) in collections {
                let is_selected = state.selected_custom_collection_id.is_none()
                    && state.selected_collection == col;
                render_nav_item(ui, label, is_selected, badge, || {
                    state.selected_collection = col;
                    state.selected_custom_collection_id = None;
                    state.selected_custom_label = None;
                    state.selected_kind_filter = None;
                });
            }

            // Dynamic rule-based collections
            for (id, label, count) in [
                ("smart_prores", "💎 ProRes Masters", 4),
                ("smart_offline", "⚠ Offline Media", 1),
            ] {
                let is_selected = state.selected_custom_collection_id.as_deref() == Some(id);
                render_nav_item(ui, label, is_selected, Some(count), || {
                    state.selected_custom_collection_id = Some(id.to_string());
                    state.selected_custom_label = Some(label.to_string());
                    state.selected_kind_filter = None;
                });
            }

            ui.add_space(8.0);
            ui.separator();

            // 4. User Reference Collections
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("USER COLLECTIONS")
                        .size(10.5)
                        .color(Color32::from_rgb(130, 145, 165))
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("＋").clicked() {}
                });
            });
            ui.add_space(2.0);

            for (id, label, count) in [
                ("col_b_roll", "🎬 Select B-Roll", 2),
                ("col_hero_takes", "⭐ Hero Takes", 1),
            ] {
                let is_selected = state.selected_custom_collection_id.as_deref() == Some(id);
                render_nav_item(ui, label, is_selected, Some(count), || {
                    state.selected_custom_collection_id = Some(id.to_string());
                    state.selected_custom_label = Some(label.to_string());
                    state.selected_kind_filter = None;
                });
            }

            ui.add_space(8.0);
            ui.separator();

            // 4. Faceted Filters by Media Type
            ui.label(
                RichText::new("FILTERS")
                    .size(10.5)
                    .color(Color32::from_rgb(130, 145, 165))
                    .strong(),
            );
            ui.add_space(2.0);

            for (kind, label) in [
                (MediaKind::Video, "Video"),
                (MediaKind::Audio, "Audio"),
                (MediaKind::Image, "Image"),
                (MediaKind::RawDng, "RAW DNG"),
                (MediaKind::ImageSequence, "Sequence"),
                (MediaKind::ProjectFile, "Project"),
            ] {
                let count = *kind_counts.get(&kind).unwrap_or(&0);
                let is_checked = state.selected_kind_filter == Some(kind);

                let mut checked = is_checked;
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut checked, label).changed() {
                        if checked {
                            state.selected_kind_filter = Some(kind);
                        } else {
                            state.selected_kind_filter = None;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{count}"))
                                .size(10.0)
                                .color(Color32::GRAY),
                        );
                    });
                });
            }
        });
}

fn render_nav_item(
    ui: &mut egui::Ui,
    label: &str,
    is_selected: bool,
    badge: Option<usize>,
    on_click: impl FnOnce(),
) {
    let text_color = if is_selected {
        Color32::from_rgb(240, 248, 255)
    } else {
        Color32::from_rgb(190, 205, 225)
    };

    let bg_color = if is_selected {
        Color32::from_rgb(28, 55, 95)
    } else {
        Color32::TRANSPARENT
    };

    let (rect, resp) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 24.0), egui::Sense::click());
    if resp.clicked() {
        on_click();
    }

    let painter = ui.painter_at(rect);
    if is_selected || resp.hovered() {
        painter.rect_filled(
            rect,
            CornerRadius::same(4),
            if is_selected {
                bg_color
            } else {
                Color32::from_rgb(28, 34, 44)
            },
        );
        if is_selected {
            painter.rect_stroke(
                rect,
                CornerRadius::same(4),
                Stroke::new(1.0, Color32::from_rgb(60, 130, 220)),
                egui::StrokeKind::Inside,
            );
        }
    }

    painter.text(
        egui::pos2(rect.min.x + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(11.5),
        text_color,
    );

    if let Some(count) = badge {
        let badge_text = format!("{count}");
        let badge_rect = Rect::from_min_size(
            egui::pos2(rect.max.x - 28.0, rect.center().y - 7.0),
            Vec2::new(22.0, 14.0),
        );
        painter.rect_filled(
            badge_rect,
            CornerRadius::same(7),
            Color32::from_rgb(45, 55, 70),
        );
        painter.text(
            badge_rect.center(),
            egui::Align2::CENTER_CENTER,
            badge_text,
            egui::FontId::proportional(9.5),
            Color32::from_rgb(200, 220, 245),
        );
    }
}
