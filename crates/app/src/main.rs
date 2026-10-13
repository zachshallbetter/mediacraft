mod app_icon;
mod automate_view;
pub mod craft_theme;
mod inspector_view;
mod library_view;
mod process_view;
mod queue_drawer;
mod queue_view;
pub mod receipts;
pub mod settings;
mod sidebar_nav;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use egui::{Color32, CornerRadius, FontId, Pos2, Rect, RichText, Stroke, Vec2};

use craft_ui::docking::{DockArea, DockStyle, Layout, Node, Permissions};
use craft_ui::layout::{SplitAxis, SplitSize};
use craft_ui::theme::WidgetPalette;

use mediacraft_catalog::MediaCatalog;
use mediacraft_daemon::QueueEngine;
use mediacraft_daemon::ipc::IpcServer;
use mediacraft_protocol::{AudioCodec, ContainerFormat, EncodingPreset, JobState, VideoCodec};

use automate_view::{AutomateViewState, render_automate_workspace};
use inspector_view::{InspectorState, render_inspector_panel};
use library_view::{LibraryViewState, render_library_view};
use process_view::{ProcessViewState, render_process_panel};
use queue_drawer::{QueueDrawerState, render_queue_drawer};
use queue_view::{QueueViewState, render_queue_workspace};
use sidebar_nav::{SidebarNavState, render_sidebar_nav};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceMode {
    Library,
    Collections,
    Process,
    Queue,
    Automations,
}

/// Docking panels in the primary workspace layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    Sidebar,
    LibraryGrid,
    Inspector,
    ProcessSidebar,
}

impl Panel {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Sidebar => "Navigator & Collections",
            Self::LibraryGrid => "Media Library",
            Self::Inspector => "Preview & Inspector",
            Self::ProcessSidebar => "Process & Output",
        }
    }
}

pub struct MediaCraftApp {
    engine: Arc<Mutex<QueueEngine>>,
    ipc_server: Option<IpcServer>,
    catalog: MediaCatalog,
    selected_asset_id: Option<String>,
    search_query: String,

    // Sub-view states
    workspace_mode: WorkspaceMode,
    library_state: LibraryViewState,
    inspector_state: InspectorState,
    process_state: ProcessViewState,
    sidebar_state: SidebarNavState,
    drawer_state: QueueDrawerState,
    queue_state: QueueViewState,
    automate_state: AutomateViewState,

    // Docking layout for main workspace
    layout: Layout<Panel>,

    // Presets catalog
    presets: Vec<EncodingPreset>,

    // Export Settings Modal
    editing_preset: Option<EncodingPreset>,
    editing_tab: usize,

    // Product branding & icon
    app_icon: egui::TextureHandle,

    // Persistence settings
    settings_store: craft_settings::SettingsStore<settings::MediaCraftPreferences>,

    last_tick: Instant,
}

impl MediaCraftApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let app_icon = cc.egui_ctx.load_texture(
            "mediacraft_app_icon",
            app_icon::app_icon_image(),
            egui::TextureOptions::LINEAR,
        );
        let settings_store = settings::open_settings();
        let mut engine = QueueEngine::new(2);

        // Preload active sample transcode jobs
        let p_proxy = EncodingPreset {
            id: "editing-proxy-1080p".to_string(),
            name: "Editing Proxy (H.264 1080p)".to_string(),
            container: ContainerFormat::Mp4,
            video_codec: VideoCodec::H264,
            width: Some(1920),
            height: Some(1080),
            frame_rate_num: Some(60000),
            frame_rate_den: Some(1001),
            video_bitrate_kbps: Some(5_000),
            audio_codec: AudioCodec::AacLc,
            audio_sample_rate: Some(48_000),
            audio_channels: Some(2),
            audio_bitrate_kbps: Some(192),
            audio_priming_samples: 1024,
            faststart: true,
        };

        let p_web = EncodingPreset {
            id: "web-delivery-1080p".to_string(),
            name: "Web Delivery (H.264 1080p Full HD)".to_string(),
            container: ContainerFormat::Mp4,
            video_codec: VideoCodec::H264,
            width: Some(1920),
            height: Some(1080),
            frame_rate_num: Some(30000),
            frame_rate_den: Some(1001),
            video_bitrate_kbps: Some(8_000),
            audio_codec: AudioCodec::AacLc,
            audio_sample_rate: Some(48_000),
            audio_channels: Some(2),
            audio_bitrate_kbps: Some(320),
            audio_priming_samples: 1024,
            faststart: true,
        };

        let j1 = engine.add_job(
            PathBuf::from("/Users/zachshallbetter/Projects/Media/A001_C001.mov"),
            PathBuf::from("/Users/zachshallbetter/Downloads/A001_C001_proxy.mp4"),
            p_proxy.clone(),
            5,
        );
        let _ = engine.update_progress(j1, 0.58, 1420, 2400, 59.94);

        let j2 = engine.add_job(
            PathBuf::from("/Users/zachshallbetter/Projects/Media/Interview_B_Roll.mp4"),
            PathBuf::from("/Users/zachshallbetter/Downloads/Interview_B_Roll_web.mp4"),
            p_web.clone(),
            4,
        );
        let _ = engine.update_progress(j2, 0.12, 108, 900, 29.97);

        let engine_arc = Arc::new(Mutex::new(engine));
        let ipc_server = IpcServer::new("/tmp/mediacraft.sock", Arc::clone(&engine_arc));
        let _ = ipc_server.start();

        // Load Standard Creative Asset Catalog & Mock Corpus
        let catalog = MediaCatalog::with_standard_mock_corpus();

        // Docking Layout: Sidebar (Left 230px), Center Grid, Inspector/Process (Right 420px)
        let center_and_right = Node::Split {
            axis: SplitAxis::Horizontal,
            size: SplitSize::FixedSecond(420.0),
            first: Box::new(Node::Tabs {
                panels: vec![Panel::LibraryGrid],
                active: 0,
            }),
            second: Box::new(Node::Split {
                axis: SplitAxis::Vertical,
                size: SplitSize::Ratio(0.55),
                first: Box::new(Node::Tabs {
                    panels: vec![Panel::Inspector],
                    active: 0,
                }),
                second: Box::new(Node::Tabs {
                    panels: vec![Panel::ProcessSidebar],
                    active: 0,
                }),
            }),
        };

        let root_node = Node::Split {
            axis: SplitAxis::Horizontal,
            size: SplitSize::FixedFirst(230.0),
            first: Box::new(Node::Tabs {
                panels: vec![Panel::Sidebar],
                active: 0,
            }),
            second: Box::new(center_and_right),
        };

        let layout = Layout {
            root: Some(root_node),
            floating: Vec::new(),
        };

        let presets = vec![
            EncodingPreset {
                id: "match-source-hq".to_string(),
                name: "Match Source - High Quality".to_string(),
                container: ContainerFormat::Mp4,
                video_codec: VideoCodec::H264,
                width: None,
                height: None,
                frame_rate_num: None,
                frame_rate_den: None,
                video_bitrate_kbps: Some(45_000),
                audio_codec: AudioCodec::AacLc,
                audio_sample_rate: Some(48_000),
                audio_channels: Some(2),
                audio_bitrate_kbps: Some(320),
                audio_priming_samples: 1024,
                faststart: true,
            },
            EncodingPreset {
                id: "web-delivery-1080p".to_string(),
                name: "Web Delivery (H.264 1080p)".to_string(),
                container: ContainerFormat::Mp4,
                video_codec: VideoCodec::H264,
                width: Some(1920),
                height: Some(1080),
                frame_rate_num: Some(30000),
                frame_rate_den: Some(1001),
                video_bitrate_kbps: Some(8_000),
                audio_codec: AudioCodec::AacLc,
                audio_sample_rate: Some(48_000),
                audio_channels: Some(2),
                audio_bitrate_kbps: Some(320),
                audio_priming_samples: 1024,
                faststart: true,
            },
            EncodingPreset {
                id: "social-media-vertical".to_string(),
                name: "Social Media (Vertical 1080x1920)".to_string(),
                container: ContainerFormat::Mp4,
                video_codec: VideoCodec::H264,
                width: Some(1080),
                height: Some(1920),
                frame_rate_num: Some(30000),
                frame_rate_den: Some(1001),
                video_bitrate_kbps: Some(6_000),
                audio_codec: AudioCodec::AacLc,
                audio_sample_rate: Some(48_000),
                audio_channels: Some(2),
                audio_bitrate_kbps: Some(192),
                audio_priming_samples: 1024,
                faststart: true,
            },
            EncodingPreset {
                id: "prores-422hq-master".to_string(),
                name: "ProRes 422 HQ (Broadcast Master)".to_string(),
                container: ContainerFormat::Mov,
                video_codec: VideoCodec::ProRes422,
                width: Some(3840),
                height: Some(2160),
                frame_rate_num: Some(24000),
                frame_rate_den: Some(1001),
                video_bitrate_kbps: Some(220_000),
                audio_codec: AudioCodec::Pcm24Le,
                audio_sample_rate: Some(48_000),
                audio_channels: Some(2),
                audio_bitrate_kbps: Some(2304),
                audio_priming_samples: 0,
                faststart: false,
            },
            EncodingPreset {
                id: "audio-only-m4a".to_string(),
                name: "Audio Only (AAC 320 kbps)".to_string(),
                container: ContainerFormat::M4a,
                video_codec: VideoCodec::None,
                width: None,
                height: None,
                frame_rate_num: None,
                frame_rate_den: None,
                video_bitrate_kbps: None,
                audio_codec: AudioCodec::AacLc,
                audio_sample_rate: Some(48_000),
                audio_channels: Some(2),
                audio_bitrate_kbps: Some(320),
                audio_priming_samples: 1024,
                faststart: true,
            },
            EncodingPreset {
                id: "editing-proxy".to_string(),
                name: "Editing Proxy (H.264 1080p 5 Mbps)".to_string(),
                container: ContainerFormat::Mp4,
                video_codec: VideoCodec::H264,
                width: Some(1920),
                height: Some(1080),
                frame_rate_num: Some(60000),
                frame_rate_den: Some(1001),
                video_bitrate_kbps: Some(5_000),
                audio_codec: AudioCodec::AacLc,
                audio_sample_rate: Some(48_000),
                audio_channels: Some(2),
                audio_bitrate_kbps: Some(192),
                audio_priming_samples: 1024,
                faststart: true,
            },
        ];

        let open_modal_initially =
            std::env::args().any(|a| a == "--modal" || a == "--export-settings");
        let initial_editing_preset = if open_modal_initially {
            presets.first().cloned()
        } else {
            None
        };

        let initial_workspace = if std::env::args().any(|a| a == "--queue" || a == "--empty-queue")
        {
            WorkspaceMode::Queue
        } else if std::env::args().any(|a| a == "--automations") {
            WorkspaceMode::Automations
        } else if std::env::args().any(|a| a == "--process") {
            WorkspaceMode::Process
        } else {
            WorkspaceMode::Library
        };

        if std::env::args().any(|a| a == "--empty-queue") {
            engine_arc
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear_all();
        }

        let mut inspector_state = InspectorState::default();
        if std::env::args().any(|a| a == "--validation") {
            inspector_state.active_tab = inspector_view::InspectorTab::Validation;
        }
        if std::env::args().any(|a| a == "--split-wipe") {
            inspector_state.preview_mode = inspector_view::PreviewMode::ComparisonSplit;
            inspector_state.split_pos = 0.52;
        }

        Self {
            engine: engine_arc,
            ipc_server: Some(ipc_server),
            catalog,
            selected_asset_id: Some("A001_C001".to_string()),
            search_query: String::new(),
            workspace_mode: initial_workspace,
            library_state: LibraryViewState::default(),
            inspector_state,
            process_state: ProcessViewState::default(),
            sidebar_state: SidebarNavState::default(),
            drawer_state: QueueDrawerState::default(),
            queue_state: QueueViewState::default(),
            automate_state: AutomateViewState::default(),
            layout,
            presets,
            editing_preset: initial_editing_preset,
            editing_tab: 0,
            app_icon,
            settings_store,
            last_tick: Instant::now(),
        }
    }

    fn engine_lock(&self) -> std::sync::MutexGuard<'_, QueueEngine> {
        self.engine.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn apply_craft_theme(&self, ui: &mut egui::Ui) {
        let palette = WidgetPalette {
            noninteractive: Color32::from_rgb(18, 21, 26),
            inactive: Color32::from_rgb(28, 33, 40),
            hovered: Color32::from_rgb(45, 54, 66),
            active: Color32::from_rgb(60, 72, 88),
            open: Color32::from_rgb(28, 33, 40),
        };
        palette.apply_to(
            &mut ui.visuals_mut().widgets,
            CornerRadius::same(3),
            Stroke::new(1.0, Color32::from_rgb(48, 56, 68)),
        );

        let visuals = ui.visuals_mut();
        visuals.override_text_color = Some(Color32::from_rgb(235, 242, 250));
    }

    fn advance_simulation(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f32();
        self.last_tick = now;

        // Poll engine progress
        let mut eng = self.engine_lock();
        let running_jobs = eng
            .list_jobs()
            .into_iter()
            .filter(|j| matches!(j.state, JobState::Running { .. }))
            .collect::<Vec<_>>();

        for job in running_jobs {
            if let JobState::Running {
                progress_pct,
                current_frame,
                ..
            } = job.state
            {
                let new_pct = (progress_pct + dt * 0.05).min(1.0);
                let new_frame = current_frame.saturating_add(2);
                if new_pct >= 1.0 {
                    let _ = eng.complete_job(job.id, 12.4, 48_000_000);
                } else {
                    let _ = eng.update_progress(job.id, new_pct, new_frame, 2400, 59.94);
                }
            }
        }
    }

    fn show_export_settings_modal(&mut self, ctx: &egui::Context) {
        let mut preset = match self.editing_preset.take() {
            Some(p) => p,
            None => return,
        };

        let mut close_modal = false;
        let mut save_modal = false;
        let mut enqueue_modal = false;

        egui::Window::new(format!("Export Settings — {}", preset.name))
            .fixed_size(Vec2::new(760.0, 520.0))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Left Column: Monitor
                    ui.allocate_ui(Vec2::new(340.0, 420.0), |ui| {
                        ui.label(RichText::new("Source / Output Preview").strong().size(11.5));
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(320.0, 180.0), egui::Sense::hover());
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(
                            rect,
                            CornerRadius::same(3),
                            Color32::from_rgb(15, 18, 22),
                        );

                        // 16:9 Test Pattern
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
                            painter.rect_filled(
                                Rect::from_min_max(
                                    Pos2::new(bx, rect.min.y),
                                    Pos2::new(bx + bar_w, rect.max.y),
                                ),
                                CornerRadius::ZERO,
                                col,
                            );
                        }

                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("01:00:14:08")
                                .size(11.0)
                                .monospace()
                                .color(Color32::from_rgb(90, 230, 130)),
                        );
                        ui.group(|ui| {
                            ui.label(
                                RichText::new(
                                    "Source Range: Entire Sequence (00:00:00:00 - 00:02:45:12)",
                                )
                                .size(10.5),
                            );
                            ui.label(
                                RichText::new("Source: 3840x2160, 23.976 fps, 48 kHz 6-ch")
                                    .size(10.0)
                                    .color(Color32::GRAY),
                            );
                        });
                        ui.label(
                            RichText::new("Estimated File Size: 937.6 MB")
                                .size(11.0)
                                .strong()
                                .color(Color32::from_rgb(80, 220, 120)),
                        );
                    });

                    ui.separator();

                    // Right Column: Settings Tabs
                    ui.allocate_ui(Vec2::new(380.0, 420.0), |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Format:").size(11.0).color(Color32::GRAY));
                            ui.label(
                                RichText::new(format!("{:?}", preset.container))
                                    .strong()
                                    .size(11.5),
                            );
                        });

                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.editing_tab, 0, "Video");
                            ui.selectable_value(&mut self.editing_tab, 1, "Audio");
                            ui.selectable_value(&mut self.editing_tab, 2, "Multiplexer");
                            ui.selectable_value(&mut self.editing_tab, 3, "Effects");
                        });
                        ui.separator();

                        match self.editing_tab {
                            0 => {
                                ui.label(RichText::new("Video Settings:").strong().size(11.0));
                                ui.label(format!("Codec: {:?}", preset.video_codec));
                                ui.horizontal(|ui| {
                                    ui.label("Resolution:");
                                    ui.label("W: 3840  H: 2160 (4K UHD)");
                                });
                                let mut br = preset.video_bitrate_kbps.unwrap_or(45000);
                                ui.add(
                                    egui::Slider::new(&mut br, 1000..=100000)
                                        .suffix(" kbps")
                                        .text("Target Bitrate"),
                                );
                                preset.video_bitrate_kbps = Some(br);
                            }
                            1 => {
                                ui.label(RichText::new("Audio Settings:").strong().size(11.0));
                                ui.label(format!("Codec: {:?}", preset.audio_codec));
                                let mut abr = preset.audio_bitrate_kbps.unwrap_or(320);
                                ui.add(
                                    egui::Slider::new(&mut abr, 64..=512)
                                        .suffix(" kbps")
                                        .text("Bitrate"),
                                );
                                preset.audio_bitrate_kbps = Some(abr);
                            }
                            2 => {
                                ui.label(
                                    RichText::new("Multiplexer & Web Streaming:")
                                        .strong()
                                        .size(11.0),
                                );
                                ui.checkbox(
                                    &mut preset.faststart,
                                    "Faststart (moov atom before mdat)",
                                );
                            }
                            _ => {
                                ui.label(
                                    RichText::new("Effects & Metadata Overlays:")
                                        .strong()
                                        .size(11.0),
                                );
                                ui.label("• Lumetri Look / LUT: None");
                                ui.label("• Burn-in Timecode: Disabled");
                            }
                        }
                    });
                });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Save & Close").clicked() {
                            save_modal = true;
                        }
                        if ui.button(RichText::new("Queue").strong()).clicked() {
                            enqueue_modal = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close_modal = true;
                        }
                    });
                });
            });

        if enqueue_modal {
            let _ = self.engine_lock().add_job(
                PathBuf::from("/Users/zachshallbetter/Projects/Media/A001_C001.mov"),
                PathBuf::from(format!(
                    "/Users/zachshallbetter/Downloads/A001_C001_{}.mp4",
                    preset.name
                )),
                preset.clone(),
                5,
            );
            self.editing_preset = None;
        } else if save_modal || close_modal {
            self.editing_preset = None;
        } else {
            self.editing_preset = Some(preset);
        }
    }
}

impl eframe::App for MediaCraftApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.advance_simulation();
        self.settings_store.tick(Instant::now());
        craft_quit::on_close_request(ctx);
        ctx.request_repaint();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.settings_store.flush();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.apply_craft_theme(ui);

        // Global hotkey: Cmd+M opens Export Settings
        if ui.input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::M)) {
            if self.editing_preset.is_none() {
                self.editing_preset = self.presets.first().cloned();
            } else {
                self.editing_preset = None;
            }
        }

        // 1. Top Application Bar
        ui.horizontal(|ui| {
            // Product Icon (mediacraft-icon.png embedded asset)
            let icon_size = Vec2::new(26.0, 26.0);
            let (icon_rect, _) = ui.allocate_exact_size(icon_size, egui::Sense::hover());
            ui.painter().image(
                self.app_icon.id(),
                icon_rect,
                Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
            ui.add_space(4.0);
            ui.heading(
                RichText::new("MediaCraft")
                    .size(17.0)
                    .color(Color32::from_rgb(240, 245, 255))
                    .strong(),
            );

            ui.add_space(12.0);

            // Workspace Mode Switcher (Library | Collections | Process | Queue | Automations)
            ui.selectable_value(
                &mut self.workspace_mode,
                WorkspaceMode::Library,
                "⊞ Library",
            );
            ui.selectable_value(
                &mut self.workspace_mode,
                WorkspaceMode::Collections,
                "📁 Collections",
            );
            ui.selectable_value(
                &mut self.workspace_mode,
                WorkspaceMode::Process,
                "⚡ Process",
            );
            ui.selectable_value(&mut self.workspace_mode, WorkspaceMode::Queue, "📋 Queue");
            ui.selectable_value(
                &mut self.workspace_mode,
                WorkspaceMode::Automations,
                "⚙ Automations",
            );

            ui.add_space(16.0);

            // Global Search Bar
            ui.label(RichText::new("🔍").size(12.0).color(Color32::GRAY));
            ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text("Search media, metadata, or content...")
                    .desired_width(240.0),
            );

            // Right Toolbar Actions using craft_ui::buttons::IconButton
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let btn_style = craft_theme::craft_icon_button_style();
                let btn_size = Vec2::new(144.0, 24.0);
                if craft_ui::buttons::IconButton::new("Export Settings", btn_size, &btn_style)
                    .selected(self.editing_preset.is_some())
                    .show(ui, |ui, rect, tint| {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "⚙ Export Settings (⌘M)",
                            egui::FontId::proportional(11.0),
                            tint,
                        );
                    })
                    .clicked()
                {
                    self.editing_preset = if self.editing_preset.is_none() {
                        self.presets.first().cloned()
                    } else {
                        None
                    };
                }

                ui.add_space(8.0);
                let q_status = self.engine_lock().status();
                let status_col = if q_status.active_jobs > 0 {
                    Color32::from_rgb(90, 230, 130)
                } else {
                    Color32::GRAY
                };
                ui.label(
                    RichText::new(format!("● IPC Active ({})", q_status.active_jobs))
                        .size(11.0)
                        .color(status_col),
                );
            });
        });

        ui.separator();

        match self.workspace_mode {
            WorkspaceMode::Queue => {
                render_queue_workspace(
                    ui,
                    &mut self.queue_state,
                    &self.engine,
                    &self.presets,
                    &mut self.editing_preset,
                );
            }
            WorkspaceMode::Automations => {
                render_automate_workspace(ui, &mut self.automate_state);
            }
            WorkspaceMode::Library | WorkspaceMode::Collections | WorkspaceMode::Process => {
                // 2. Central Docking Workspace Area
                let mut style = DockStyle::from_ui(ui);
                style.tab_height = 26.0;
                style.background = Color32::from_rgb(16, 19, 24);
                style.tab_background = Color32::from_rgb(25, 30, 38);
                style.active_background = Color32::from_rgb(42, 50, 64);
                style.text = Color32::from_rgb(235, 245, 255);
                style.inactive_text = Color32::from_rgb(160, 175, 195);
                style.accent = Color32::from_rgb(80, 190, 255);
                style.font = FontId::proportional(11.5);
                style.border = Stroke::new(1.0, Color32::from_rgb(45, 52, 65));

                let mut layout = std::mem::take(&mut self.layout);

                let filtered_assets = self.catalog.query(
                    self.sidebar_state.selected_collection,
                    &self.search_query,
                    self.sidebar_state.selected_kind_filter,
                );

                let selected_asset = self
                    .selected_asset_id
                    .as_deref()
                    .and_then(|id| self.catalog.get_by_id(id));

                let mut enqueue_job_action: Option<(EncodingPreset, String, String)> = None;

                let output = DockArea::new(egui::Id::new("mediacraft_workspace_dock")).show(
                    ui,
                    &layout,
                    &style,
                    |panel| panel.label().into(),
                    |_| Permissions::default(),
                    |ui, panel| match *panel {
                        Panel::Sidebar => {
                            let total = self.catalog.all_assets().len();
                            let counts = self.catalog.kind_counts();
                            render_sidebar_nav(ui, &mut self.sidebar_state, &counts, total);
                        }
                        Panel::LibraryGrid => {
                            render_library_view(
                                ui,
                                &mut self.library_state,
                                self.sidebar_state.selected_collection,
                                &mut self.search_query,
                                &filtered_assets,
                                &mut self.selected_asset_id,
                            );
                        }
                        Panel::Inspector => {
                            render_inspector_panel(
                                ui,
                                &mut self.inspector_state,
                                selected_asset,
                                self.catalog.graph(),
                            );
                        }
                        Panel::ProcessSidebar => {
                            render_process_panel(
                                ui,
                                &mut self.process_state,
                                selected_asset,
                                &self.presets,
                                &mut enqueue_job_action,
                            );
                        }
                    },
                );

                for action in output.actions {
                    let _ = layout.apply(action);
                }
                self.layout = layout;

                // Handle enqueue action from process panel
                if let Some((preset, src, out)) = enqueue_job_action {
                    let _ = self.engine_lock().add_job(
                        PathBuf::from(src),
                        PathBuf::from(out),
                        preset,
                        5,
                    );
                    self.drawer_state.is_expanded = true;
                }

                // 3. Bottom Activity Drawer (Queue & Watch Folders)
                ui.separator();
                render_queue_drawer(ui, &mut self.drawer_state, &self.engine);
            }
        }

        // 4. Modal Export Settings Window (if triggered)
        self.show_export_settings_modal(ui.ctx());
    }
}

impl Drop for MediaCraftApp {
    fn drop(&mut self) {
        if let Some(ref server) = self.ipc_server {
            server.stop();
        }
    }
}

fn main() -> Result<(), eframe::Error> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(Vec2::new(1400.0, 840.0))
            .with_min_inner_size(Vec2::new(960.0, 600.0))
            .with_title("MediaCraft — Universal Media Workspace & Encoding Pipeline")
            .with_icon(app_icon::window_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "MediaCraft",
        native_options,
        Box::new(|cc| Ok(Box::new(MediaCraftApp::new(cc)))),
    )
}
