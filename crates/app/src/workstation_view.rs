//! Workstation View (Matching Mockup `media_1791864376895.jpg`)
//!
//! Provides the complete, cohesive desktop media encoding workstation interface:
//! 1. Window Menu & Top Action Toolbar (Add Media, Add Folder, Remove, Start Queue, Pause, Stop, Auto-Encode)
//! 2. Left Panel: Media Browser with Tree (Favorites/Locations) + Thumbnail Grid with zoom slider & view toggles
//! 3. Center Top Panel: Queue Table with thumbnails, format/preset pills, status badges, and progress meters
//! 4. Center Bottom Panel: 16:9 Video Preview Player + Scrubber + Transport Controls + Tabbed Media Info sheet
//! 5. Right Panel: Format Settings Inspector (Video, Audio, Output, Collapsible Metadata/Color/Advanced)
//! 6. Bottom Status Bar: Status indicator and batch transcode summary

use crate::mockup_art::{WorkstationTextures, paint_clip_thumbnail_with_textures};
use egui::{Color32, CornerRadius, FontId, Pos2, Rect, RichText, Stroke, Vec2};

#[derive(Debug, Clone)]
pub struct WorkstationQueueItem {
    pub id: u64,
    pub checked: bool,
    pub filename: String,
    pub resolution: String,
    pub fps: String,
    pub duration: String,
    pub format_display: String,
    pub container_display: String,
    pub preset_name: String,
    pub preset_desc: String,
    pub output_path: String,
    pub is_encoding: bool,
    pub progress: f32,
    pub time_remaining: String,
}

#[derive(Debug, Clone)]
pub struct BrowserMediaClip {
    pub filename: String,
    pub duration: String,
    pub resolution: String,
}

pub struct WorkstationState {
    // Top bar
    pub auto_encode: bool,
    pub is_queue_running: bool,
    pub active_workspace: String,

    // Left Panel (Media Browser)
    pub active_left_tab: usize, // 0 = Media Browser, 1 = Watch Folders, 2 = History
    pub selected_nav_item: String,
    pub browser_grid_mode: bool, // true = grid, false = list
    pub thumbnail_zoom: f32,
    pub browser_clips: Vec<BrowserMediaClip>,
    pub selected_browser_clip: usize,

    // Center Top (Queue)
    pub active_queue_tab: usize, // 0 = Queue (6), 1 = Encoding (1), 2 = Completed (2)
    pub selected_queue_row: usize,
    pub queue_select_all: bool,
    pub queue_items: Vec<WorkstationQueueItem>,

    // Center Bottom (Preview & Info)
    pub active_preview_tab: usize, // 0 = Preview, 1 = Media Info, 2 = Activity, 3 = Log
    pub active_info_tab: usize,    // 0 = Source, 1 = Video, 2 = Audio, 3 = Metadata
    pub playback_time_secs: f32,
    pub is_playing: bool,
    pub fit_mode: String,

    // Right Panel (Format Settings)
    pub active_inspector_tab: usize, // 0 = Presets, 1 = Format Settings, 2 = Output
    pub video_expanded: bool,
    pub audio_expanded: bool,
    pub output_expanded: bool,
    pub metadata_expanded: bool,
    pub color_expanded: bool,
    pub advanced_expanded: bool,

    // Video settings
    pub video_format: String,
    pub video_container: String,
    pub video_resolution: String,
    pub video_framerate: String,
    pub video_bitrate_mode: String,
    pub target_bitrate_mbps: u32,
    pub video_profile: String,
    pub video_level: String,
    pub hardware_encoding: bool,
    pub hardware_encoder: String,

    // Audio settings
    pub audio_codec: String,
    pub audio_sample_rate: String,
    pub audio_channels: String,
    pub audio_bitrate_kbps: u32,

    // Output settings
    pub output_filename: String,
    pub output_location: String,
    pub overwrite_policy: String,
    pub create_subfolder: bool,
    pub applied_preset: String,
    pub textures: WorkstationTextures,
}

impl Default for WorkstationState {
    fn default() -> Self {
        let queue_items = vec![
            WorkstationQueueItem {
                id: 1,
                checked: true,
                filename: "A001_C012.mov".to_string(),
                resolution: "3840 x 2160".to_string(),
                fps: "23.976 fps".to_string(),
                duration: "00:00:12:04".to_string(),
                format_display: "H.264".to_string(),
                container_display: "MP4".to_string(),
                preset_name: "YouTube 4K".to_string(),
                preset_desc: "H.264 • 20 Mbps".to_string(),
                output_path: ".../Exports/A001_C012.mp4".to_string(),
                is_encoding: true,
                progress: 0.42,
                time_remaining: "00:01:23 remaining".to_string(),
            },
            WorkstationQueueItem {
                id: 2,
                checked: false,
                filename: "Concert_Footage.mov".to_string(),
                resolution: "1920 x 1080".to_string(),
                fps: "29.97 fps".to_string(),
                duration: "00:02:34:16".to_string(),
                format_display: "H.265".to_string(),
                container_display: "MP4".to_string(),
                preset_name: "Vimeo 1080p".to_string(),
                preset_desc: "H.265 • 10 Mbps".to_string(),
                output_path: ".../Exports/Concert_Footage.mp4".to_string(),
                is_encoding: false,
                progress: 0.0,
                time_remaining: "—".to_string(),
            },
            WorkstationQueueItem {
                id: 3,
                checked: false,
                filename: "Drone_Shot_4K.mov".to_string(),
                resolution: "3840 x 2160".to_string(),
                fps: "23.976 fps".to_string(),
                duration: "00:00:28:12".to_string(),
                format_display: "ProRes".to_string(),
                container_display: "MOV".to_string(),
                preset_name: "ProRes 422 HQ".to_string(),
                preset_desc: "ProRes 422 • 10-bit".to_string(),
                output_path: ".../Exports/Drone_Shot_4K.mov".to_string(),
                is_encoding: false,
                progress: 0.0,
                time_remaining: "—".to_string(),
            },
            WorkstationQueueItem {
                id: 4,
                checked: false,
                filename: "Interview_A7S3.mov".to_string(),
                resolution: "3840 x 2160".to_string(),
                fps: "23.976 fps".to_string(),
                duration: "00:05:10:00".to_string(),
                format_display: "H.264".to_string(),
                container_display: "MP4".to_string(),
                preset_name: "Podcast 1080p".to_string(),
                preset_desc: "H.264 • 8 Mbps".to_string(),
                output_path: ".../Exports/Interview_A7S3.mp4".to_string(),
                is_encoding: false,
                progress: 0.0,
                time_remaining: "—".to_string(),
            },
            WorkstationQueueItem {
                id: 5,
                checked: false,
                filename: "City_Timelapse.mov".to_string(),
                resolution: "3840 x 2160".to_string(),
                fps: "23.976 fps".to_string(),
                duration: "00:00:15:00".to_string(),
                format_display: "H.265".to_string(),
                container_display: "MP4".to_string(),
                preset_name: "Social Media 4K".to_string(),
                preset_desc: "H.265 • 15 Mbps".to_string(),
                output_path: ".../Exports/City_Timelapse.mp4".to_string(),
                is_encoding: false,
                progress: 0.0,
                time_remaining: "—".to_string(),
            },
            WorkstationQueueItem {
                id: 6,
                checked: false,
                filename: "Underwater_01.mov".to_string(),
                resolution: "1920 x 1080".to_string(),
                fps: "29.97 fps".to_string(),
                duration: "00:00:24:00".to_string(),
                format_display: "AAC".to_string(),
                container_display: "M4A (Audio)".to_string(),
                preset_name: "Audio High Quality".to_string(),
                preset_desc: "AAC • 320 kbps".to_string(),
                output_path: ".../Exports/Underwater_01.m4a".to_string(),
                is_encoding: false,
                progress: 0.0,
                time_remaining: "—".to_string(),
            },
        ];

        let browser_clips = vec![
            BrowserMediaClip {
                filename: "A001_C012.mov".to_string(),
                duration: "00:00:12:04".to_string(),
                resolution: "3840 x 2160".to_string(),
            },
            BrowserMediaClip {
                filename: "Concert_Footage.mov".to_string(),
                duration: "00:02:34:16".to_string(),
                resolution: "1920 x 1080".to_string(),
            },
            BrowserMediaClip {
                filename: "Drone_Shot_4K.mov".to_string(),
                duration: "00:00:28:12".to_string(),
                resolution: "3840 x 2160".to_string(),
            },
            BrowserMediaClip {
                filename: "Interview_A7S3.mov".to_string(),
                duration: "00:05:10:00".to_string(),
                resolution: "3840 x 2160".to_string(),
            },
            BrowserMediaClip {
                filename: "City_Timelapse.mov".to_string(),
                duration: "00:00:15:00".to_string(),
                resolution: "3840 x 2160".to_string(),
            },
            BrowserMediaClip {
                filename: "Underwater_01.mov".to_string(),
                duration: "00:00:24:00".to_string(),
                resolution: "1920 x 1080".to_string(),
            },
        ];

        Self {
            auto_encode: true,
            is_queue_running: true,
            active_workspace: "Default Workspace".to_string(),

            active_left_tab: 0,
            selected_nav_item: "Movies".to_string(),
            browser_grid_mode: true,
            thumbnail_zoom: 0.5,
            browser_clips,
            selected_browser_clip: 0,

            active_queue_tab: 0,
            selected_queue_row: 0,
            queue_select_all: false,
            queue_items,

            active_preview_tab: 0,
            active_info_tab: 0,
            playback_time_secs: 0.0,
            is_playing: false,
            fit_mode: "Fit".to_string(),

            active_inspector_tab: 1, // Format Settings
            video_expanded: true,
            audio_expanded: true,
            output_expanded: true,
            metadata_expanded: false,
            color_expanded: false,
            advanced_expanded: false,

            video_format: "H.264 (AVC)".to_string(),
            video_container: "MP4".to_string(),
            video_resolution: "3840 x 2160 (4K UHD)".to_string(),
            video_framerate: "Match Source (23.976 fps)".to_string(),
            video_bitrate_mode: "VBR, Target".to_string(),
            target_bitrate_mbps: 20,
            video_profile: "High".to_string(),
            video_level: "5.1".to_string(),
            hardware_encoding: true,
            hardware_encoder: "Auto".to_string(),

            audio_codec: "AAC (LC)".to_string(),
            audio_sample_rate: "48,000 Hz".to_string(),
            audio_channels: "Stereo".to_string(),
            audio_bitrate_kbps: 320,

            output_filename: "A001_C012.mp4".to_string(),
            output_location: ".../Movies/Exports".to_string(),
            overwrite_policy: "Ask".to_string(),
            create_subfolder: true,
            applied_preset: "YouTube 4K".to_string(),
            textures: WorkstationTextures::default(),
        }
    }
}

pub fn render_workstation(
    ui: &mut egui::Ui,
    state: &mut WorkstationState,
    preferences_open: &mut bool,
) {
    state.textures.ensure_loaded(ui.ctx());

    // 1. Top Menus and Header Toolbar
    render_top_menu_and_actions(ui, state, preferences_open);

    ui.add_space(2.0);

    // 2. Main 4-Quadrant Workstation Layout
    let full_rect = ui.available_rect_before_wrap();
    let status_bar_h = 22.0;
    let quadrant_rect = Rect::from_min_max(
        full_rect.min,
        Pos2::new(full_rect.max.x, full_rect.max.y - status_bar_h - 2.0),
    );
    let status_bar_rect = Rect::from_min_max(
        Pos2::new(full_rect.min.x, full_rect.max.y - status_bar_h),
        full_rect.max,
    );

    let total_width = quadrant_rect.width();
    let total_height = quadrant_rect.height();

    let left_width = 280.0_f32.min(total_width * 0.20).max(255.0);
    let right_width = 350.0_f32.min(total_width * 0.26).max(325.0);
    let center_width = (total_width - left_width - right_width - 8.0).max(320.0);

    let queue_height = (total_height * 0.42).clamp(260.0, 320.0);
    let preview_height = total_height - queue_height - 6.0;

    let mut quad_ui = ui.new_child(egui::UiBuilder::new().max_rect(quadrant_rect));
    quad_ui.horizontal(|ui| {
        // --- LEFT COLUMN: Media Browser ---
        ui.allocate_ui_with_layout(
            Vec2::new(left_width, total_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                render_left_panel(ui, state, left_width, total_height);
            },
        );

        ui.add_space(3.0);

        // --- CENTER COLUMN: Queue (Top) + Preview/Inspector (Bottom) ---
        ui.allocate_ui_with_layout(
            Vec2::new(center_width, total_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                // Center Top: Queue
                render_queue_panel(ui, state, center_width, queue_height);

                ui.add_space(4.0);

                // Center Bottom: Preview & Media Info
                render_preview_and_info_panel(ui, state, center_width, preview_height);
            },
        );

        ui.add_space(3.0);

        // --- RIGHT COLUMN: Format Settings Inspector ---
        ui.allocate_ui_with_layout(
            Vec2::new(right_width, total_height),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                render_right_inspector_panel(ui, state, right_width, total_height);
            },
        );
    });

    // 3. Bottom Status Bar (anchored flush at the window bottom)
    let mut status_ui = ui.new_child(egui::UiBuilder::new().max_rect(status_bar_rect));
    render_bottom_status_bar(&mut status_ui, state);

    ui.allocate_rect(full_rect, egui::Sense::hover());
}

// ----------------------------------------------------------------------------
// Top Menu & Action Toolbar
// ----------------------------------------------------------------------------

fn render_top_menu_and_actions(
    ui: &mut egui::Ui,
    state: &mut WorkstationState,
    preferences_open: &mut bool,
) {
    // 1. Integrated Window Header & Menu Bar (Mac Traffic Light Clearance on Left)
    let bar_h = 30.0;
    let (bar_rect, bar_resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), bar_h),
        egui::Sense::click_and_drag(),
    );

    if bar_resp.dragged() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }

    ui.painter()
        .rect_filled(bar_rect, CornerRadius::ZERO, Color32::from_rgb(16, 19, 25));
    ui.painter().line_segment(
        [Pos2::new(bar_rect.min.x, bar_rect.max.y), bar_rect.max],
        Stroke::new(1.0, Color32::from_rgb(32, 38, 48)),
    );

    let mut header_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar_rect));
    header_ui.horizontal(|ui| {
        #[cfg(target_os = "macos")]
        {
            ui.add_space(78.0);
        }
        #[cfg(not(target_os = "macos"))]
        {
            ui.add_space(8.0);
        }

        let (icon_rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), egui::Sense::hover());
        let painter = ui.painter_at(icon_rect);
        painter.rect_filled(
            icon_rect,
            CornerRadius::same(4),
            Color32::from_rgb(130, 60, 230),
        );
        painter.text(
            icon_rect.center(),
            egui::Align2::CENTER_CENTER,
            "M",
            FontId::proportional(13.0),
            Color32::WHITE,
        );

        ui.add_space(4.0);
        ui.label(
            RichText::new("MediaCraft")
                .size(13.5)
                .strong()
                .color(Color32::WHITE),
        );

        ui.add_space(14.0);

        let menus = ["File", "Edit", "Queue", "Preset", "View", "Window", "Help"];
        for m in menus {
            let label = RichText::new(m)
                .size(11.5)
                .color(Color32::from_rgb(195, 205, 222));
            let _ = ui.add(egui::Button::new(label).frame(false));
            ui.add_space(2.0);
        }

        // Right side of Top Bar: Workspace selector, split icons, settings gear
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let icon_col = Color32::from_rgb(160, 175, 195);

            if ui
                .button(RichText::new("⚙").size(14.0).color(icon_col))
                .clicked()
            {
                *preferences_open = !*preferences_open;
            }

            ui.add_space(4.0);
            let _ = ui.button(RichText::new("[≡]").size(12.0).color(icon_col));
            let _ = ui.button(RichText::new("[|]").size(12.0).color(icon_col));

            ui.add_space(8.0);
            egui::ComboBox::from_id_salt("workstation_workspace_selector")
                .selected_text(RichText::new(&state.active_workspace).size(11.5))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut state.active_workspace,
                        "Default Workspace".to_string(),
                        "Default Workspace",
                    );
                    ui.selectable_value(
                        &mut state.active_workspace,
                        "Encoding Queue".to_string(),
                        "Encoding Queue",
                    );
                    ui.selectable_value(
                        &mut state.active_workspace,
                        "Media Browser".to_string(),
                        "Media Browser",
                    );
                });
        });
    });

    ui.add_space(2.0);

    // 2. Action Toolbar: Add Media, Add Folder, Remove | Start Queue, Pause, Stop | Auto-Encode
    ui.horizontal(|ui| {
        let btn_bg = Color32::from_rgb(28, 32, 40);
        let stroke = Stroke::new(1.0, Color32::from_rgb(48, 56, 70));

        let add_btn = egui::Button::new(
            RichText::new("＋ Add Media...")
                .size(11.5)
                .color(Color32::from_rgb(220, 230, 245)),
        )
        .fill(btn_bg)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(3));
        let _ = ui.add(add_btn);

        let folder_btn = egui::Button::new(
            RichText::new("📁 Add Folder...")
                .size(11.5)
                .color(Color32::from_rgb(220, 230, 245)),
        )
        .fill(btn_bg)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(3));
        let _ = ui.add(folder_btn);

        let remove_btn = egui::Button::new(
            RichText::new("🗑 Remove")
                .size(11.5)
                .color(Color32::from_rgb(180, 195, 215)),
        )
        .fill(btn_bg)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(3));
        let _ = ui.add(remove_btn);

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        // Start Queue button (emerald green accent from mockup)
        let (start_col, start_text) = if state.is_queue_running {
            (Color32::from_rgb(16, 185, 129), "▶ Start Queue")
        } else {
            (Color32::from_rgb(16, 160, 115), "▶ Start Queue")
        };
        let start_btn = egui::Button::new(
            RichText::new(start_text)
                .size(11.5)
                .strong()
                .color(Color32::WHITE),
        )
        .fill(start_col)
        .stroke(Stroke::new(1.0, Color32::from_rgb(34, 197, 94)))
        .corner_radius(CornerRadius::same(3));
        if ui.add(start_btn).clicked() {
            state.is_queue_running = !state.is_queue_running;
        }

        let pause_btn = egui::Button::new(
            RichText::new("⏸ Pause")
                .size(11.5)
                .color(Color32::from_rgb(200, 215, 235)),
        )
        .fill(btn_bg)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(3));
        if ui.add(pause_btn).clicked() {
            state.is_queue_running = false;
        }

        let stop_btn = egui::Button::new(
            RichText::new("⏹ Stop")
                .size(11.5)
                .color(Color32::from_rgb(200, 215, 235)),
        )
        .fill(btn_bg)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(3));
        if ui.add(stop_btn).clicked() {
            state.is_queue_running = false;
        }

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        // Auto-Encode toggle
        ui.checkbox(
            &mut state.auto_encode,
            RichText::new("Auto-Encode").size(11.5),
        );
    });
}

// ----------------------------------------------------------------------------
// Left Panel: Media Browser (Tree on Top, Thumbnails on Bottom)
// ----------------------------------------------------------------------------

fn render_left_panel(ui: &mut egui::Ui, state: &mut WorkstationState, width: f32, height: f32) {
    crate::craft_theme::panel_card_frame()
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(width - 4.0);
            ui.set_height(height - 4.0);

            // Tab bar: Media Browser | Watch Folders | History
            ui.horizontal(|ui| {
                let tabs = ["Media Browser", "Watch Folders", "History"];
                for (idx, name) in tabs.iter().enumerate() {
                    let is_active = state.active_left_tab == idx;
                    let text_color = if is_active {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(140, 155, 175)
                    };
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(82.0, 24.0), egui::Sense::click());
                    if is_active {
                        ui.painter().rect_filled(
                            Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - 2.0), rect.max),
                            CornerRadius::ZERO,
                            Color32::from_rgb(45, 140, 240),
                        );
                    }
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        *name,
                        FontId::proportional(11.5),
                        text_color,
                    );
                    if resp.clicked() {
                        state.active_left_tab = idx;
                    }
                }
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // Tree navigation area (Favorites, Locations)
            let tree_height = (height * 0.28).clamp(110.0, 180.0);
            egui::ScrollArea::vertical()
                .id_salt("browser_tree_scroll")
                .max_height(tree_height)
                .show(ui, |ui| {
                    render_browser_tree(ui, state);
                });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // Bottom toolbar: [⊞] [≡] zoom slider [⛶]
            ui.horizontal(|ui| {
                let active_grid = state.browser_grid_mode;
                let grid_btn =
                    egui::Button::new(RichText::new("⊞").size(13.0)).fill(if active_grid {
                        Color32::from_rgb(45, 140, 240)
                    } else {
                        Color32::from_rgb(32, 36, 45)
                    });
                if ui.add(grid_btn).clicked() {
                    state.browser_grid_mode = true;
                }

                let list_btn =
                    egui::Button::new(RichText::new("≡").size(13.0)).fill(if !active_grid {
                        Color32::from_rgb(45, 140, 240)
                    } else {
                        Color32::from_rgb(32, 36, 45)
                    });
                if ui.add(list_btn).clicked() {
                    state.browser_grid_mode = false;
                }

                ui.add_space(6.0);
                ui.add(egui::Slider::new(&mut state.thumbnail_zoom, 0.2..=1.0).show_value(false));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let _ = ui.button(RichText::new("⛶").size(12.0));
                });
            });

            ui.add_space(4.0);

            // Thumbnail grid area
            let thumb_h = ui.available_height().max(60.0);
            egui::ScrollArea::vertical()
                .id_salt("browser_thumbnails_scroll")
                .max_height(thumb_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    render_browser_clip_grid(ui, state);
                });
        });
}

fn render_browser_tree(ui: &mut egui::Ui, state: &mut WorkstationState) {
    ui.label(
        RichText::new("▼ ★ Favorites")
            .size(11.0)
            .color(Color32::from_rgb(160, 175, 195)),
    );
    let favorites = ["Desktop", "Documents", "Movies", "Music", "Pictures"];
    for name in favorites {
        let is_selected = state.selected_nav_item == name;
        let bg = if is_selected {
            Color32::from_rgb(26, 68, 128)
        } else {
            Color32::TRANSPARENT
        };
        let text_col = if is_selected {
            Color32::WHITE
        } else {
            Color32::from_rgb(210, 220, 235)
        };

        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), egui::Sense::click());
        if is_selected {
            ui.painter().rect_filled(rect, CornerRadius::same(3), bg);
        }
        let icon = match name {
            "Desktop" => "🖥",
            "Documents" => "📄",
            "Movies" => "🎬",
            "Music" => "🎵",
            _ => "🖼",
        };
        let label = format!("  {icon}  {name}");
        ui.painter().text(
            Pos2::new(rect.min.x + 8.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(11.0),
            text_col,
        );
        if resp.clicked() {
            state.selected_nav_item = name.to_string();
        }
    }

    ui.add_space(4.0);
    ui.label(
        RichText::new("▼ Locations")
            .size(11.0)
            .color(Color32::from_rgb(160, 175, 195)),
    );
    let locations = [
        "Macintosh HD",
        "Media Drive",
        "Projects",
        "Exports",
        "Network",
    ];
    for name in locations {
        let is_selected = state.selected_nav_item == name;
        let bg = if is_selected {
            Color32::from_rgb(26, 68, 128)
        } else {
            Color32::TRANSPARENT
        };
        let text_col = if is_selected {
            Color32::WHITE
        } else {
            Color32::from_rgb(210, 220, 235)
        };

        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), egui::Sense::click());
        if is_selected {
            ui.painter().rect_filled(rect, CornerRadius::same(3), bg);
        }
        let icon = if name == "Network" { "🌐" } else { "📁" };
        let label = format!("  {icon}  {name}");
        ui.painter().text(
            Pos2::new(rect.min.x + 8.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(11.0),
            text_col,
        );
        if resp.clicked() {
            state.selected_nav_item = name.to_string();
        }
    }
}

fn render_browser_clip_grid(ui: &mut egui::Ui, state: &mut WorkstationState) {
    let card_w = (112.0 * (0.8 + state.thumbnail_zoom * 0.4)).max(85.0);
    let thumb_h = card_w * (9.0 / 16.0);
    let card_h = thumb_h + 26.0;

    ui.horizontal_wrapped(|ui| {
        for (idx, clip) in state.browser_clips.clone().iter().enumerate() {
            let is_sel = state.selected_browser_clip == idx;
            let (rect, resp) =
                ui.allocate_exact_size(Vec2::new(card_w, card_h), egui::Sense::click());

            let bg = if is_sel {
                Color32::from_rgb(28, 48, 76)
            } else {
                Color32::from_rgb(20, 23, 30)
            };
            let stroke = if is_sel {
                Stroke::new(1.5, Color32::from_rgb(45, 140, 240))
            } else {
                Stroke::new(1.0, Color32::from_rgb(38, 44, 56))
            };

            ui.painter().rect_filled(rect, CornerRadius::same(4), bg);
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(4),
                stroke,
                egui::StrokeKind::Inside,
            );

            // Thumbnail image area (strict 16:9 widescreen)
            let thumb_rect = Rect::from_min_size(rect.min, Vec2::new(card_w, thumb_h));
            paint_clip_thumbnail_with_textures(
                ui.painter(),
                &state.textures,
                thumb_rect,
                &clip.filename,
            );

            // Duration badge over thumbnail
            let badge_rect = Rect::from_min_size(
                Pos2::new(thumb_rect.min.x + 3.0, thumb_rect.max.y - 13.0),
                Vec2::new(50.0, 11.0),
            );
            ui.painter().rect_filled(
                badge_rect,
                CornerRadius::same(2),
                Color32::from_black_alpha(190),
            );
            ui.painter().text(
                badge_rect.center(),
                egui::Align2::CENTER_CENTER,
                &clip.duration,
                FontId::monospace(8.0),
                Color32::WHITE,
            );

            // Text label under thumbnail
            let text_rect = Rect::from_min_max(
                Pos2::new(rect.min.x + 4.0, thumb_rect.max.y + 2.0),
                Pos2::new(rect.max.x - 4.0, rect.max.y - 1.0),
            );
            ui.painter().text(
                Pos2::new(text_rect.min.x, text_rect.min.y + 5.0),
                egui::Align2::LEFT_CENTER,
                &clip.filename,
                FontId::proportional(10.0),
                Color32::from_rgb(230, 240, 255),
            );
            ui.painter().text(
                Pos2::new(text_rect.min.x, text_rect.min.y + 16.0),
                egui::Align2::LEFT_CENTER,
                &clip.resolution,
                FontId::proportional(8.5),
                Color32::from_rgb(130, 145, 165),
            );

            if resp.clicked() {
                state.selected_browser_clip = idx;
                // Sync selected clip with queue
                if let Some(pos) = state
                    .queue_items
                    .iter()
                    .position(|q| q.filename == clip.filename)
                {
                    state.selected_queue_row = pos;
                }
            }
        }
    });
}

// ----------------------------------------------------------------------------
// Center Top: Encoding Queue Table
// ----------------------------------------------------------------------------

fn render_queue_panel(ui: &mut egui::Ui, state: &mut WorkstationState, width: f32, height: f32) {
    crate::craft_theme::panel_card_frame()
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(width - 4.0);
            ui.set_height(height - 4.0);

            // Tab bar: Queue (6) | Encoding (1) | Completed (2)
            ui.horizontal(|ui| {
                let tabs = ["Queue (6)", "Encoding (1)", "Completed (2)"];
                for (idx, name) in tabs.iter().enumerate() {
                    let is_active = state.active_queue_tab == idx;
                    let text_color = if is_active {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(140, 155, 175)
                    };
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(96.0, 24.0), egui::Sense::click());
                    if is_active {
                        ui.painter().rect_filled(
                            Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - 2.0), rect.max),
                            CornerRadius::ZERO,
                            Color32::from_rgb(45, 140, 240),
                        );
                    }
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        *name,
                        FontId::proportional(12.0),
                        text_color,
                    );
                    if resp.clicked() {
                        state.active_queue_tab = idx;
                    }
                }
            });

            ui.add_space(4.0);
            ui.separator();

            // Table Columns layout helper
            let compute_queue_cols = |r: Rect| {
                let min_x = r.min.x;
                let max_x = r.max.x;
                let check_x = min_x + 10.0;
                let num_x = min_x + 28.0;
                let thumb_x = min_x + 44.0;
                let source_text_x = min_x + 108.0;
                let format_x = min_x + 218.0;
                let format_w = 64.0;
                let preset_x = min_x + 288.0;
                let preset_w = 88.0;
                let menu_x = max_x - 14.0;
                let prog_x = max_x - 120.0;
                let status_x = max_x - 195.0;
                let output_x = preset_x + preset_w + 14.0;
                let output_max_w = (status_x - output_x - 10.0).max(10.0);
                (
                    check_x,
                    num_x,
                    thumb_x,
                    source_text_x,
                    format_x,
                    format_w,
                    preset_x,
                    preset_w,
                    output_x,
                    output_max_w,
                    status_x,
                    prog_x,
                    menu_x,
                )
            };

            // Table Header: [ ] | # | Source | Format | Preset | Output File | Status | Progress | ≡
            let (header_rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 20.0), egui::Sense::hover());
            let (
                h_check_x,
                h_num_x,
                h_thumb_x,
                _h_src_x,
                h_format_x,
                _h_format_w,
                h_preset_x,
                _h_preset_w,
                h_output_x,
                _h_output_max_w,
                h_status_x,
                h_prog_x,
                h_menu_x,
            ) = compute_queue_cols(header_rect);

            let header_painter = ui.painter();
            let header_color = Color32::from_rgb(140, 155, 175);
            let header_font = FontId::proportional(11.0);

            // Header Checkbox
            let check_rect = Rect::from_center_size(
                Pos2::new(h_check_x + 6.0, header_rect.center().y),
                Vec2::splat(12.0),
            );
            header_painter.rect_stroke(
                check_rect,
                CornerRadius::same(2),
                Stroke::new(1.0, header_color),
                egui::StrokeKind::Inside,
            );
            if state.queue_select_all {
                header_painter.rect_filled(
                    check_rect.shrink(2.0),
                    CornerRadius::same(1),
                    Color32::from_rgb(0, 180, 245),
                );
            }
            if ui
                .interact(check_rect, ui.id().with("hdr_chk"), egui::Sense::click())
                .clicked()
            {
                state.queue_select_all = !state.queue_select_all;
                for item in &mut state.queue_items {
                    item.checked = state.queue_select_all;
                }
            }

            header_painter.text(
                Pos2::new(h_num_x, header_rect.center().y),
                egui::Align2::LEFT_CENTER,
                "#",
                header_font.clone(),
                header_color,
            );
            header_painter.text(
                Pos2::new(h_thumb_x, header_rect.center().y),
                egui::Align2::LEFT_CENTER,
                "Source",
                header_font.clone(),
                header_color,
            );
            header_painter.text(
                Pos2::new(h_format_x, header_rect.center().y),
                egui::Align2::LEFT_CENTER,
                "Format",
                header_font.clone(),
                header_color,
            );
            header_painter.text(
                Pos2::new(h_preset_x, header_rect.center().y),
                egui::Align2::LEFT_CENTER,
                "Preset",
                header_font.clone(),
                header_color,
            );
            header_painter.text(
                Pos2::new(h_output_x, header_rect.center().y),
                egui::Align2::LEFT_CENTER,
                "Output File",
                header_font.clone(),
                header_color,
            );
            header_painter.text(
                Pos2::new(h_status_x, header_rect.center().y),
                egui::Align2::LEFT_CENTER,
                "Status",
                header_font.clone(),
                header_color,
            );
            header_painter.text(
                Pos2::new(h_prog_x, header_rect.center().y),
                egui::Align2::LEFT_CENTER,
                "Progress",
                header_font.clone(),
                header_color,
            );
            header_painter.text(
                Pos2::new(h_menu_x, header_rect.center().y),
                egui::Align2::CENTER_CENTER,
                "≡",
                header_font,
                header_color,
            );

            ui.add_space(2.0);
            ui.separator();

            // Table Body
            let body_h = ui.available_height().max(60.0);
            egui::ScrollArea::vertical()
                .id_salt("queue_table_scroll")
                .max_height(body_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (row_idx, item) in state.queue_items.clone().iter().enumerate() {
                        let is_selected = state.selected_queue_row == row_idx;
                        let row_h = 48.0;
                        let (rect, resp) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), row_h),
                            egui::Sense::click(),
                        );

                        let row_bg = if is_selected {
                            Color32::from_rgb(26, 48, 76) // Deep blue selection from mockup
                        } else if row_idx % 2 == 0 {
                            Color32::from_rgb(22, 25, 32)
                        } else {
                            Color32::from_rgb(19, 22, 28)
                        };

                        ui.painter()
                            .rect_filled(rect, CornerRadius::same(2), row_bg);
                        if is_selected {
                            ui.painter().rect_stroke(
                                rect,
                                CornerRadius::same(2),
                                Stroke::new(1.0, Color32::from_rgb(45, 110, 195)),
                                egui::StrokeKind::Inside,
                            );
                        }

                        let (
                            col_check_x,
                            col_num_x,
                            col_thumb_x,
                            col_src_x,
                            col_format_x,
                            col_format_w,
                            col_preset_x,
                            col_preset_w,
                            col_output_x,
                            col_output_max_w,
                            col_status_x,
                            col_prog_x,
                            col_menu_x,
                        ) = compute_queue_cols(rect);

                        // Checkbox
                        let chk_center = Pos2::new(col_check_x + 6.0, rect.center().y);
                        let mut checked = item.checked;
                        if ui
                            .put(
                                Rect::from_center_size(chk_center, Vec2::new(16.0, 16.0)),
                                egui::Checkbox::without_text(&mut checked),
                            )
                            .changed()
                        {
                            state.queue_items[row_idx].checked = checked;
                        }

                        // Row number (#)
                        ui.painter().text(
                            Pos2::new(col_num_x, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            format!("{}", item.id),
                            FontId::proportional(11.0),
                            Color32::from_rgb(140, 155, 175),
                        );

                        // Thumbnail (16:9 56x31.5)
                        let thumb_rect = Rect::from_min_size(
                            Pos2::new(col_thumb_x, rect.min.y + 8.0),
                            Vec2::new(56.0, 31.5),
                        );
                        paint_clip_thumbnail_with_textures(
                            ui.painter(),
                            &state.textures,
                            thumb_rect,
                            &item.filename,
                        );

                        // Source details: Filename (top line), resolution + fps (bottom line)
                        let source_w = (col_format_x - col_src_x - 8.0).max(10.0);
                        let mut display_name = item.filename.clone();
                        let max_chars = ((source_w / 6.5) as usize).max(8);
                        if display_name.chars().count() > max_chars {
                            let head: String = display_name
                                .chars()
                                .take(max_chars.saturating_sub(3))
                                .collect();
                            display_name = format!("{head}...");
                        }
                        ui.painter().text(
                            Pos2::new(col_src_x, rect.min.y + 14.0),
                            egui::Align2::LEFT_CENTER,
                            display_name,
                            FontId::proportional(11.5),
                            Color32::WHITE,
                        );
                        ui.painter().text(
                            Pos2::new(col_src_x, rect.min.y + 32.0),
                            egui::Align2::LEFT_CENTER,
                            format!("{}  {}", item.resolution, item.fps),
                            FontId::proportional(10.0),
                            Color32::from_rgb(130, 145, 165),
                        );

                        // Format Combobox Pill
                        let pill_rect = Rect::from_min_size(
                            Pos2::new(col_format_x, rect.min.y + 10.0),
                            Vec2::new(col_format_w, 28.0),
                        );
                        ui.painter().rect_filled(
                            pill_rect,
                            CornerRadius::same(3),
                            Color32::from_rgb(32, 36, 45),
                        );
                        ui.painter().rect_stroke(
                            pill_rect,
                            CornerRadius::same(3),
                            Stroke::new(1.0, Color32::from_rgb(50, 56, 70)),
                            egui::StrokeKind::Inside,
                        );
                        ui.painter().text(
                            Pos2::new(pill_rect.center().x, pill_rect.min.y + 9.0),
                            egui::Align2::CENTER_CENTER,
                            format!("{} ▾", item.format_display),
                            FontId::proportional(10.0),
                            Color32::WHITE,
                        );
                        ui.painter().text(
                            Pos2::new(pill_rect.center().x, pill_rect.min.y + 20.0),
                            egui::Align2::CENTER_CENTER,
                            &item.container_display,
                            FontId::proportional(8.5),
                            Color32::from_rgb(140, 155, 175),
                        );

                        // Preset Pill
                        let preset_rect = Rect::from_min_size(
                            Pos2::new(col_preset_x, rect.min.y + 10.0),
                            Vec2::new(col_preset_w, 28.0),
                        );
                        ui.painter().rect_filled(
                            preset_rect,
                            CornerRadius::same(3),
                            Color32::from_rgb(32, 36, 45),
                        );
                        ui.painter().rect_stroke(
                            preset_rect,
                            CornerRadius::same(3),
                            Stroke::new(1.0, Color32::from_rgb(50, 56, 70)),
                            egui::StrokeKind::Inside,
                        );
                        ui.painter().text(
                            Pos2::new(preset_rect.center().x, preset_rect.min.y + 9.0),
                            egui::Align2::CENTER_CENTER,
                            &item.preset_name,
                            FontId::proportional(10.0),
                            Color32::WHITE,
                        );
                        ui.painter().text(
                            Pos2::new(preset_rect.center().x, preset_rect.min.y + 20.0),
                            egui::Align2::CENTER_CENTER,
                            &item.preset_desc,
                            FontId::proportional(8.5),
                            Color32::from_rgb(140, 155, 175),
                        );

                        // Output File hyperlink with dynamic truncation
                        let mut output_str = item.output_path.clone();
                        let char_cap = ((col_output_max_w / 6.2) as usize).max(6);
                        if output_str.chars().count() > char_cap {
                            let head: String = output_str
                                .chars()
                                .take(char_cap.saturating_sub(3))
                                .collect();
                            output_str = format!("{head}...");
                        }
                        ui.painter().text(
                            Pos2::new(col_output_x, rect.center().y),
                            egui::Align2::LEFT_CENTER,
                            output_str,
                            FontId::proportional(10.5),
                            Color32::from_rgb(60, 150, 230),
                        );

                        // Status Badge: Encoding (cyan) or Waiting (gray)
                        if item.is_encoding {
                            ui.painter().text(
                                Pos2::new(col_status_x, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                "Encoding",
                                FontId::proportional(11.0),
                                Color32::from_rgb(0, 195, 255), // Cyan
                            );
                        } else {
                            ui.painter().text(
                                Pos2::new(col_status_x, rect.center().y),
                                egui::Align2::LEFT_CENTER,
                                "Waiting",
                                FontId::proportional(11.0),
                                Color32::from_rgb(130, 145, 165),
                            );
                        }

                        // Progress: Bar + Time Remaining
                        if item.is_encoding {
                            let prog_bar_w = 46.0;
                            let prog_rect = Rect::from_min_size(
                                Pos2::new(col_prog_x, rect.min.y + 12.0),
                                Vec2::new(prog_bar_w, 6.0),
                            );
                            ui.painter().rect_filled(
                                prog_rect,
                                CornerRadius::same(3),
                                Color32::from_rgb(40, 48, 60),
                            );
                            let fill_w = prog_rect.width() * item.progress;
                            let fill_rect =
                                Rect::from_min_size(prog_rect.min, Vec2::new(fill_w, 6.0));
                            ui.painter().rect_filled(
                                fill_rect,
                                CornerRadius::same(3),
                                Color32::from_rgb(0, 180, 245),
                            );

                            // Percentage & time remaining text
                            ui.painter().text(
                                Pos2::new(prog_rect.max.x + 6.0, rect.min.y + 15.0),
                                egui::Align2::LEFT_CENTER,
                                format!("{:.0}%", item.progress * 100.0),
                                FontId::proportional(10.0),
                                Color32::WHITE,
                            );
                            ui.painter().text(
                                Pos2::new(col_prog_x, rect.min.y + 30.0),
                                egui::Align2::LEFT_CENTER,
                                &item.time_remaining,
                                FontId::proportional(8.5),
                                Color32::from_rgb(140, 155, 175),
                            );
                        } else {
                            ui.painter().text(
                                Pos2::new(col_prog_x + 20.0, rect.center().y),
                                egui::Align2::CENTER_CENTER,
                                "—",
                                FontId::proportional(12.0),
                                Color32::from_rgb(100, 115, 135),
                            );
                        }

                        // Context menu handle ≡
                        ui.painter().text(
                            Pos2::new(col_menu_x, rect.center().y),
                            egui::Align2::CENTER_CENTER,
                            "≡",
                            FontId::proportional(13.0),
                            Color32::from_rgb(110, 125, 145),
                        );

                        if resp.clicked() {
                            state.selected_queue_row = row_idx;
                        }
                    }
                });
        });
}

// ----------------------------------------------------------------------------
// Center Bottom: Video Preview & Detailed Media Info
// ----------------------------------------------------------------------------

fn render_preview_and_info_panel(
    ui: &mut egui::Ui,
    state: &mut WorkstationState,
    width: f32,
    height: f32,
) {
    crate::craft_theme::panel_card_frame()
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(width - 4.0);
            ui.set_height(height - 4.0);

            // Tab bar: Preview | Media Info | Activity | Log
            ui.horizontal(|ui| {
                let tabs = ["Preview", "Media Info", "Activity", "Log"];
                for (idx, name) in tabs.iter().enumerate() {
                    let is_active = state.active_preview_tab == idx;
                    let text_color = if is_active {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(140, 155, 175)
                    };
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(76.0, 22.0), egui::Sense::click());
                    if is_active {
                        ui.painter().rect_filled(
                            Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - 2.0), rect.max),
                            CornerRadius::ZERO,
                            Color32::from_rgb(45, 140, 240),
                        );
                    }
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        *name,
                        FontId::proportional(11.5),
                        text_color,
                    );
                    if resp.clicked() {
                        state.active_preview_tab = idx;
                    }
                }
            });

            ui.add_space(2.0);
            ui.separator();

            let avail_w = ui.available_width();
            let avail_h = ui.available_height();

            // Split into Video Player (Left 56%) and Media Info Sheet (Right 44%)
            let player_w = (avail_w * 0.55).max(240.0);
            let info_w = avail_w - player_w - 12.0;

            ui.horizontal(|ui| {
                // --- Video Player Preview ---
                ui.allocate_ui(Vec2::new(player_w, avail_h), |ui| {
                    render_video_player(ui, state, player_w, avail_h);
                });

                ui.separator();

                // --- Media Info Property Sheet ---
                ui.allocate_ui(Vec2::new(info_w, avail_h), |ui| {
                    render_media_info_sheet(ui, state, info_w, avail_h);
                });
            });
        });
}

fn render_video_player(ui: &mut egui::Ui, state: &mut WorkstationState, width: f32, height: f32) {
    ui.vertical(|ui| {
        // Strict 16:9 Cinema Canvas calculation
        let avail_w = (width - 4.0).max(40.0);
        let avail_h = (height - 54.0).max(40.0);
        let avail_bounds = Rect::from_min_size(ui.cursor().min, Vec2::new(avail_w, avail_h));
        let fitted = crate::mockup_art::fit_aspect_rect(avail_bounds, 16.0 / 9.0);
        let frame_w = fitted.width();
        let pad_x = (avail_w - fitted.width()).max(0.0) * 0.5;
        let pad_y = (avail_h - fitted.height()).max(0.0) * 0.5;

        if pad_y > 1.0 {
            ui.add_space(pad_y);
        }

        let canvas_rect = ui
            .horizontal(|ui| {
                if pad_x > 1.0 {
                    ui.add_space(pad_x);
                }
                let (rect, _) = ui.allocate_exact_size(fitted.size(), egui::Sense::hover());
                rect
            })
            .inner;

        let active_clip_name = state
            .queue_items
            .get(state.selected_queue_row)
            .map(|q| q.filename.as_str())
            .unwrap_or("A001_C012.mov");

        if let Some(tex) = state.textures.get_preview_texture(active_clip_name) {
            let uv = crate::mockup_art::calculate_cover_uv(tex.size(), canvas_rect);
            ui.painter()
                .image(tex.id(), canvas_rect, uv, Color32::WHITE);
            ui.painter().rect_stroke(
                canvas_rect,
                CornerRadius::same(2),
                Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 30)),
                egui::StrokeKind::Inside,
            );
        } else {
            paint_clip_thumbnail_with_textures(
                ui.painter(),
                &state.textures,
                canvas_rect,
                active_clip_name,
            );
        }

        ui.add_space(3.0);

        // Scrubber line: [00:00:00:00]  ---o-----------------  [00:00:12:04]
        ui.horizontal(|ui| {
            if pad_x > 1.0 {
                ui.add_space(pad_x);
            }

            ui.label(
                RichText::new("00:00:00:00")
                    .size(10.5)
                    .monospace()
                    .color(Color32::from_rgb(0, 195, 255)),
            );

            let scrubber_w = (frame_w - 160.0).max(60.0);
            let (scrub_rect, scrub_resp) =
                ui.allocate_exact_size(Vec2::new(scrubber_w, 14.0), egui::Sense::click_and_drag());
            let line_y = scrub_rect.center().y;
            // Track
            ui.painter().line_segment(
                [
                    Pos2::new(scrub_rect.min.x, line_y),
                    Pos2::new(scrub_rect.max.x, line_y),
                ],
                Stroke::new(2.5, Color32::from_rgb(45, 52, 65)),
            );
            // Filled range
            let fill_x = scrub_rect.min.x
                + scrub_rect.width() * (state.playback_time_secs / 12.0).clamp(0.0, 1.0);
            ui.painter().line_segment(
                [
                    Pos2::new(scrub_rect.min.x, line_y),
                    Pos2::new(fill_x, line_y),
                ],
                Stroke::new(2.5, Color32::from_rgb(45, 140, 240)),
            );
            // Playhead thumb
            ui.painter()
                .circle_filled(Pos2::new(fill_x, line_y), 4.5, Color32::WHITE);

            if (scrub_resp.clicked() || scrub_resp.dragged())
                && let Some(pos) = scrub_resp.interact_pointer_pos()
            {
                let norm = ((pos.x - scrub_rect.min.x) / scrub_rect.width()).clamp(0.0, 1.0);
                state.playback_time_secs = norm * 12.0;
            }

            ui.label(
                RichText::new("00:00:12:04")
                    .size(10.5)
                    .monospace()
                    .color(Color32::from_rgb(0, 195, 255)),
            );
        });

        ui.add_space(2.0);

        // Transport buttons: |◀  ◀◀  ▶  ▶▶  ▶|    Fit ▾    [⛶]
        ui.horizontal(|ui| {
            if pad_x > 1.0 {
                ui.add_space(pad_x);
            }
            ui.add_space((frame_w * 0.18).max(0.0));
            let btn_col = Color32::from_rgb(180, 195, 215);

            let _ = ui.button(RichText::new("|◀").size(11.0).color(btn_col));
            let _ = ui.button(RichText::new("◀◀").size(11.0).color(btn_col));
            let play_text = if state.is_playing { "⏸" } else { "▶" };
            if ui
                .button(
                    RichText::new(play_text)
                        .size(12.0)
                        .strong()
                        .color(Color32::WHITE),
                )
                .clicked()
            {
                state.is_playing = !state.is_playing;
            }
            let _ = ui.button(RichText::new("▶▶").size(11.0).color(btn_col));
            let _ = ui.button(RichText::new("▶|").size(11.0).color(btn_col));

            ui.add_space(20.0);
            egui::ComboBox::from_id_salt("workstation_fit_selector")
                .selected_text(RichText::new(&state.fit_mode).size(10.5))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut state.fit_mode, "Fit".to_string(), "Fit");
                    ui.selectable_value(&mut state.fit_mode, "100%".to_string(), "100%");
                    ui.selectable_value(&mut state.fit_mode, "50%".to_string(), "50%");
                });

            let _ = ui.button(RichText::new("⛶").size(11.0).color(btn_col));
        });
    });
}

fn render_media_info_sheet(
    ui: &mut egui::Ui,
    state: &mut WorkstationState,
    _width: f32,
    _height: f32,
) {
    ui.vertical(|ui| {
        // Sub-tabs: Source | Video | Audio | Metadata
        ui.horizontal(|ui| {
            let tabs = ["Source", "Video", "Audio", "Metadata"];
            for (idx, name) in tabs.iter().enumerate() {
                let is_sel = state.active_info_tab == idx;
                let text = if is_sel {
                    RichText::new(*name)
                        .size(11.0)
                        .strong()
                        .color(Color32::WHITE)
                } else {
                    RichText::new(*name)
                        .size(11.0)
                        .color(Color32::from_rgb(130, 145, 165))
                };
                if ui.selectable_label(is_sel, text).clicked() {
                    state.active_info_tab = idx;
                }
            }
        });

        ui.add_space(2.0);
        ui.separator();
        ui.add_space(2.0);

        let active_clip = state
            .queue_items
            .get(state.selected_queue_row)
            .cloned()
            .unwrap_or_else(|| state.queue_items[0].clone());

        // Key-Value Table matching mockup
        let metadata_rows = [
            ("File Name:", active_clip.filename.clone()),
            ("Format:", "QuickTime (MOV)".to_string()),
            (
                "Video:",
                format!("{} ({})", active_clip.resolution, active_clip.fps),
            ),
            ("Codec:", "Apple ProRes 422".to_string()),
            ("Pixel Format:", "yuv422p10le".to_string()),
            ("Audio:", "6 channels (48 kHz)".to_string()),
            ("Duration:", active_clip.duration.clone()),
            ("File Size:", "1.2 GB".to_string()),
            ("Color Profile:", "Rec. 709".to_string()),
            ("Bit Depth:", "10-bit".to_string()),
            ("Alpha:", "None".to_string()),
            (
                "Location:",
                format!("/Volumes/Media/Movies/{}", active_clip.filename),
            ),
        ];

        let sheet_h = ui.available_height().max(60.0);
        egui::ScrollArea::vertical()
            .id_salt("media_info_sheet_scroll")
            .max_height(sheet_h)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("media_info_key_val_grid")
                    .num_columns(2)
                    .spacing(Vec2::new(14.0, 4.0))
                    .striped(true)
                    .show(ui, |ui| {
                        for (k, v) in metadata_rows {
                            ui.label(
                                RichText::new(k)
                                    .size(10.5)
                                    .color(Color32::from_rgb(130, 145, 165)),
                            );
                            ui.label(
                                RichText::new(v)
                                    .size(10.5)
                                    .color(Color32::from_rgb(220, 230, 245)),
                            );
                            ui.end_row();
                        }
                    });
            });
    });
}

// ----------------------------------------------------------------------------
// Right Panel: Format Settings Inspector
// ----------------------------------------------------------------------------

fn render_right_inspector_panel(
    ui: &mut egui::Ui,
    state: &mut WorkstationState,
    width: f32,
    height: f32,
) {
    crate::craft_theme::panel_card_frame()
        .inner_margin(egui::Margin::same(8))
        .show(ui, |ui| {
            ui.set_width(width - 4.0);
            ui.set_height(height - 4.0);

            // Tab bar: Presets | Format Settings | Output
            ui.horizontal(|ui| {
                let tabs = ["Presets", "Format Settings", "Output"];
                for (idx, name) in tabs.iter().enumerate() {
                    let is_active = state.active_inspector_tab == idx;
                    let text_color = if is_active {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(140, 155, 175)
                    };
                    let (rect, resp) =
                        ui.allocate_exact_size(Vec2::new(96.0, 24.0), egui::Sense::click());
                    if is_active {
                        ui.painter().rect_filled(
                            Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - 2.0), rect.max),
                            CornerRadius::ZERO,
                            Color32::from_rgb(45, 140, 240),
                        );
                    }
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        *name,
                        FontId::proportional(11.5),
                        text_color,
                    );
                    if resp.clicked() {
                        state.active_inspector_tab = idx;
                    }
                }
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // Scrollable inspector contents
            let bottom_controls_h = 104.0;
            let scroll_h = (ui.available_height() - bottom_controls_h).max(60.0);
            egui::ScrollArea::vertical()
                .id_salt("inspector_settings_scroll")
                .max_height(scroll_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    render_inspector_collapsible_groups(ui, state);
                });

            ui.add_space(3.0);
            ui.separator();
            ui.add_space(3.0);

            // Bottom controls: Apply Preset + Save / Save As / Reset buttons
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Apply Preset:")
                        .size(10.5)
                        .color(Color32::from_rgb(140, 155, 175)),
                );
                egui::ComboBox::from_id_salt("bottom_apply_preset_selector")
                    .selected_text(RichText::new(&state.applied_preset).size(10.5))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut state.applied_preset,
                            "YouTube 4K".to_string(),
                            "YouTube 4K",
                        );
                        ui.selectable_value(
                            &mut state.applied_preset,
                            "Vimeo 1080p".to_string(),
                            "Vimeo 1080p",
                        );
                        ui.selectable_value(
                            &mut state.applied_preset,
                            "ProRes 422 HQ".to_string(),
                            "ProRes 422 HQ",
                        );
                    });
            });

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                let btn_bg = Color32::from_rgb(32, 37, 46);
                let stroke = Stroke::new(1.0, Color32::from_rgb(50, 58, 72));
                let btn_w = ((ui.available_width() - 12.0) / 3.0).max(55.0);
                let _ = ui.add_sized(
                    Vec2::new(btn_w, 22.0),
                    egui::Button::new(RichText::new("Save...").size(11.0))
                        .fill(btn_bg)
                        .stroke(stroke),
                );
                let _ = ui.add_sized(
                    Vec2::new(btn_w, 22.0),
                    egui::Button::new(RichText::new("Save As...").size(11.0))
                        .fill(btn_bg)
                        .stroke(stroke),
                );
                let _ = ui.add_sized(
                    Vec2::new(btn_w, 22.0),
                    egui::Button::new(RichText::new("Reset").size(11.0))
                        .fill(btn_bg)
                        .stroke(stroke),
                );
            });
        });
}

fn render_inspector_collapsible_groups(ui: &mut egui::Ui, state: &mut WorkstationState) {
    // ▼ Video Section
    let video_header = if state.video_expanded {
        "▼ Video"
    } else {
        "▶ Video"
    };
    if ui
        .button(
            RichText::new(video_header)
                .strong()
                .size(12.0)
                .color(Color32::WHITE),
        )
        .clicked()
    {
        state.video_expanded = !state.video_expanded;
    }
    if state.video_expanded {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.vertical(|ui| {
                egui::Grid::new("video_settings_grid")
                    .num_columns(2)
                    .spacing(Vec2::new(10.0, 6.0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("Format")
                                .size(10.5)
                                .color(Color32::from_rgb(140, 155, 175)),
                        );
                        egui::ComboBox::from_id_salt("fmt_combo")
                            .selected_text(&state.video_format)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.video_format,
                                    "H.264 (AVC)".to_string(),
                                    "H.264 (AVC)",
                                );
                                ui.selectable_value(
                                    &mut state.video_format,
                                    "HEVC (H.265)".to_string(),
                                    "HEVC (H.265)",
                                );
                                ui.selectable_value(
                                    &mut state.video_format,
                                    "Apple ProRes".to_string(),
                                    "Apple ProRes",
                                );
                            });
                        ui.end_row();

                        ui.label(
                            RichText::new("Container")
                                .size(10.5)
                                .color(Color32::from_rgb(140, 155, 175)),
                        );
                        egui::ComboBox::from_id_salt("container_combo")
                            .selected_text(&state.video_container)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.video_container,
                                    "MP4".to_string(),
                                    "MP4",
                                );
                                ui.selectable_value(
                                    &mut state.video_container,
                                    "MOV".to_string(),
                                    "MOV",
                                );
                            });
                        ui.end_row();

                        ui.label(
                            RichText::new("Resolution")
                                .size(10.5)
                                .color(Color32::from_rgb(140, 155, 175)),
                        );
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("res_combo")
                                .selected_text(&state.video_resolution)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut state.video_resolution,
                                        "3840 x 2160 (4K UHD)".to_string(),
                                        "3840 x 2160 (4K UHD)",
                                    );
                                    ui.selectable_value(
                                        &mut state.video_resolution,
                                        "1920 x 1080 (1080p)".to_string(),
                                        "1920 x 1080 (1080p)",
                                    );
                                });
                            ui.label("🔗");
                        });
                        ui.end_row();

                        ui.label(
                            RichText::new("Frame Rate")
                                .size(10.5)
                                .color(Color32::from_rgb(140, 155, 175)),
                        );
                        egui::ComboBox::from_id_salt("fps_combo")
                            .selected_text(&state.video_framerate)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.video_framerate,
                                    "Match Source (23.976 fps)".to_string(),
                                    "Match Source (23.976 fps)",
                                );
                                ui.selectable_value(
                                    &mut state.video_framerate,
                                    "24 fps".to_string(),
                                    "24 fps",
                                );
                                ui.selectable_value(
                                    &mut state.video_framerate,
                                    "29.97 fps".to_string(),
                                    "29.97 fps",
                                );
                                ui.selectable_value(
                                    &mut state.video_framerate,
                                    "60 fps".to_string(),
                                    "60 fps",
                                );
                            });
                        ui.end_row();

                        ui.label(
                            RichText::new("Bitrate Mode")
                                .size(10.5)
                                .color(Color32::from_rgb(140, 155, 175)),
                        );
                        egui::ComboBox::from_id_salt("bitrate_mode_combo")
                            .selected_text(&state.video_bitrate_mode)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.video_bitrate_mode,
                                    "VBR, Target".to_string(),
                                    "VBR, Target",
                                );
                                ui.selectable_value(
                                    &mut state.video_bitrate_mode,
                                    "CBR".to_string(),
                                    "CBR",
                                );
                            });
                        ui.end_row();

                        ui.label(
                            RichText::new("Target Bitrate")
                                .size(10.5)
                                .color(Color32::from_rgb(140, 155, 175)),
                        );
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut state.target_bitrate_mbps).range(1..=150),
                            );
                            ui.label("Mbps");
                        });
                        ui.end_row();

                        ui.label(
                            RichText::new("Profile")
                                .size(10.5)
                                .color(Color32::from_rgb(140, 155, 175)),
                        );
                        egui::ComboBox::from_id_salt("profile_combo")
                            .selected_text(&state.video_profile)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.video_profile,
                                    "High".to_string(),
                                    "High",
                                );
                                ui.selectable_value(
                                    &mut state.video_profile,
                                    "Main".to_string(),
                                    "Main",
                                );
                            });
                        ui.end_row();

                        ui.label(
                            RichText::new("Level")
                                .size(10.5)
                                .color(Color32::from_rgb(140, 155, 175)),
                        );
                        egui::ComboBox::from_id_salt("level_combo")
                            .selected_text(&state.video_level)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.video_level,
                                    "5.1".to_string(),
                                    "5.1",
                                );
                                ui.selectable_value(
                                    &mut state.video_level,
                                    "5.2".to_string(),
                                    "5.2",
                                );
                            });
                        ui.end_row();
                    });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.checkbox(
                        &mut state.hardware_encoding,
                        RichText::new("Hardware Encoding").size(10.5),
                    );
                    egui::ComboBox::from_id_salt("hw_encoder_combo")
                        .selected_text(&state.hardware_encoder)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut state.hardware_encoder,
                                "Auto".to_string(),
                                "Auto",
                            );
                            ui.selectable_value(
                                &mut state.hardware_encoder,
                                "Metal / VideoToolbox".to_string(),
                                "Metal / VideoToolbox",
                            );
                        });
                });
            });
        });
    }

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    // ▼ Audio Section
    let audio_header = if state.audio_expanded {
        "▼ Audio"
    } else {
        "▶ Audio"
    };
    if ui
        .button(
            RichText::new(audio_header)
                .strong()
                .size(12.0)
                .color(Color32::WHITE),
        )
        .clicked()
    {
        state.audio_expanded = !state.audio_expanded;
    }
    if state.audio_expanded {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.vertical(|ui| {
                egui::Grid::new("audio_settings_grid")
                    .num_columns(2)
                    .spacing(Vec2::new(10.0, 6.0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("Codec")
                                .size(10.5)
                                .color(Color32::from_rgb(140, 155, 175)),
                        );
                        egui::ComboBox::from_id_salt("audio_codec_combo")
                            .selected_text(&state.audio_codec)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.audio_codec,
                                    "AAC (LC)".to_string(),
                                    "AAC (LC)",
                                );
                                ui.selectable_value(
                                    &mut state.audio_codec,
                                    "PCM 24-bit".to_string(),
                                    "PCM 24-bit",
                                );
                                ui.selectable_value(
                                    &mut state.audio_codec,
                                    "FLAC".to_string(),
                                    "FLAC",
                                );
                            });
                        ui.end_row();

                        ui.label(
                            RichText::new("Sample Rate")
                                .size(10.5)
                                .color(Color32::from_rgb(140, 155, 175)),
                        );
                        egui::ComboBox::from_id_salt("audio_sr_combo")
                            .selected_text(&state.audio_sample_rate)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.audio_sample_rate,
                                    "48,000 Hz".to_string(),
                                    "48,000 Hz",
                                );
                                ui.selectable_value(
                                    &mut state.audio_sample_rate,
                                    "44,100 Hz".to_string(),
                                    "44,100 Hz",
                                );
                                ui.selectable_value(
                                    &mut state.audio_sample_rate,
                                    "96,000 Hz".to_string(),
                                    "96,000 Hz",
                                );
                            });
                        ui.end_row();

                        ui.label(
                            RichText::new("Channels")
                                .size(10.5)
                                .color(Color32::from_rgb(140, 155, 175)),
                        );
                        egui::ComboBox::from_id_salt("audio_ch_combo")
                            .selected_text(&state.audio_channels)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut state.audio_channels,
                                    "Stereo".to_string(),
                                    "Stereo",
                                );
                                ui.selectable_value(
                                    &mut state.audio_channels,
                                    "5.1 Surround".to_string(),
                                    "5.1 Surround",
                                );
                            });
                        ui.end_row();

                        ui.label(
                            RichText::new("Bitrate")
                                .size(10.5)
                                .color(Color32::from_rgb(140, 155, 175)),
                        );
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::DragValue::new(&mut state.audio_bitrate_kbps).range(64..=512),
                            );
                            ui.label("kbps");
                        });
                        ui.end_row();
                    });
            });
        });
    }

    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);

    // ▼ Output Section
    let output_header = if state.output_expanded {
        "▼ Output"
    } else {
        "▶ Output"
    };
    if ui
        .button(
            RichText::new(output_header)
                .strong()
                .size(12.0)
                .color(Color32::WHITE),
        )
        .clicked()
    {
        state.output_expanded = !state.output_expanded;
    }
    if state.output_expanded {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("File Name")
                        .size(10.5)
                        .color(Color32::from_rgb(140, 155, 175)),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut state.output_filename)
                        .desired_width(ui.available_width()),
                );

                ui.add_space(4.0);
                ui.label(
                    RichText::new("Location")
                        .size(10.5)
                        .color(Color32::from_rgb(140, 155, 175)),
                );
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut state.output_location)
                            .desired_width(ui.available_width() - 80.0),
                    );
                    let _ = ui.button("Browse...");
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Overwrite")
                            .size(10.5)
                            .color(Color32::from_rgb(140, 155, 175)),
                    );
                    egui::ComboBox::from_id_salt("overwrite_combo")
                        .selected_text(&state.overwrite_policy)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut state.overwrite_policy,
                                "Ask".to_string(),
                                "Ask",
                            );
                            ui.selectable_value(
                                &mut state.overwrite_policy,
                                "Overwrite".to_string(),
                                "Overwrite",
                            );
                        });
                });

                ui.checkbox(
                    &mut state.create_subfolder,
                    RichText::new("Create Subfolder from Source Name").size(10.5),
                );
            });
        });
    }

    ui.add_space(6.0);
    ui.separator();

    // Collapsed Sections: Metadata, Color, Advanced
    let meta_hdr = if state.metadata_expanded {
        "▼ Metadata"
    } else {
        "▶ Metadata"
    };
    if ui
        .button(
            RichText::new(meta_hdr)
                .size(11.0)
                .color(Color32::from_rgb(160, 175, 195)),
        )
        .clicked()
    {
        state.metadata_expanded = !state.metadata_expanded;
    }

    let color_hdr = if state.color_expanded {
        "▼ Color"
    } else {
        "▶ Color"
    };
    if ui
        .button(
            RichText::new(color_hdr)
                .size(11.0)
                .color(Color32::from_rgb(160, 175, 195)),
        )
        .clicked()
    {
        state.color_expanded = !state.color_expanded;
    }

    let adv_hdr = if state.advanced_expanded {
        "▼ Advanced"
    } else {
        "▶ Advanced"
    };
    if ui
        .button(
            RichText::new(adv_hdr)
                .size(11.0)
                .color(Color32::from_rgb(160, 175, 195)),
        )
        .clicked()
    {
        state.advanced_expanded = !state.advanced_expanded;
    }
}

// ----------------------------------------------------------------------------
// Bottom Status Bar
// ----------------------------------------------------------------------------

fn render_bottom_status_bar(ui: &mut egui::Ui, _state: &mut WorkstationState) {
    let (bar_rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(bar_rect, CornerRadius::ZERO, Color32::from_rgb(16, 18, 24));
    ui.painter().line_segment(
        [bar_rect.min, Pos2::new(bar_rect.max.x, bar_rect.min.y)],
        Stroke::new(1.0, Color32::from_rgb(32, 36, 46)),
    );

    // Left: Green status dot + "Ready"
    let dot_center = Pos2::new(bar_rect.min.x + 14.0, bar_rect.center().y);
    ui.painter()
        .circle_filled(dot_center, 3.5, Color32::from_rgb(34, 197, 94));
    ui.painter().text(
        Pos2::new(dot_center.x + 8.0, bar_rect.center().y),
        egui::Align2::LEFT_CENTER,
        "Ready",
        FontId::proportional(11.0),
        Color32::from_rgb(210, 225, 240),
    );

    // Right: "1 encoding | 5 waiting | Estimated time remaining: 00:08:21"
    ui.painter().text(
        Pos2::new(bar_rect.max.x - 14.0, bar_rect.center().y),
        egui::Align2::RIGHT_CENTER,
        "1 encoding   |   5 waiting   |   Estimated time remaining: 00:08:21",
        FontId::proportional(11.0),
        Color32::from_rgb(130, 145, 165),
    );
}
