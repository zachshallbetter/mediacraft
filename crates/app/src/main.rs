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

use mediacraft_catalog::{MediaAsset, MediaCatalog};
use mediacraft_daemon::QueueEngine;
use mediacraft_daemon::ipc::IpcServer;
use mediacraft_protocol::{
    AudioCodec, ContainerFormat, DeliveryContract, EncodingPreset, JobState, VideoCodec,
};

use automate_view::{AutomateViewState, render_automate_workspace};
mod deliver_view;
use deliver_view::{DeliverViewState, render_deliver_workspace};
use inspector_view::{InspectorState, render_inspector_panel};
use library_view::{LibraryViewContext, LibraryViewState, render_library_view};
mod preset_view;
use preset_view::{PresetViewState, PresetWorkspaceTab, render_preset_workspace};
use process_view::{ProcessViewState, render_process_panel};
use queue_drawer::{QueueDrawerState, render_queue_drawer};
use queue_view::{QueueViewState, render_queue_workspace};
use sidebar_nav::{SidebarNavState, render_sidebar_nav};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceMode {
    Browse,
    Inspect,
    Collections,
    Process,
    Queue,
    Automations,
    Deliver,
    Presets,
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
    watch_folder_engine: mediacraft_daemon::watch_folder::WatchFolderEngine,
    catalog: MediaCatalog,
    selected_asset_id: Option<String>,
    search_query: String,
    collection_manager: mediacraft_catalog::CollectionManager,

    // Sub-view states
    workspace_mode: WorkspaceMode,
    library_state: LibraryViewState,
    inspector_state: InspectorState,
    process_state: ProcessViewState,
    sidebar_state: SidebarNavState,
    drawer_state: QueueDrawerState,
    queue_state: QueueViewState,
    automate_state: AutomateViewState,
    deliver_state: DeliverViewState,
    preset_state: PresetViewState,

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
    preferences_open: bool,

    // Safe termination lifecycle via craft-quit
    show_quit_confirm: bool,
    quit_confirmed: bool,

    last_tick: Instant,
}

impl MediaCraftApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let _ = craft_quit::install_app_menu(&cc.egui_ctx, "MediaCraft");
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
            contract: Some(DeliveryContract::default()),
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
            contract: Some(DeliveryContract::default()),
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
                contract: Some(DeliveryContract::default()),
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
                contract: Some(DeliveryContract::default()),
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
                contract: Some(DeliveryContract::default()),
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
                contract: Some(DeliveryContract {
                    name: "Broadcast Master Contract".to_string(),
                    require_faststart: false,
                    max_bitrate_kbps: Some(250_000),
                    max_true_peak_dbfs: Some(-1.0),
                    target_lufs: Some(-24.0),
                    lufs_tolerance: Some(1.0),
                    require_audio_priming: false,
                    max_file_size_bytes: None,
                }),
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
                contract: Some(DeliveryContract {
                    name: "AAC Audio Streaming Contract".to_string(),
                    require_faststart: true,
                    max_bitrate_kbps: Some(350),
                    max_true_peak_dbfs: Some(-1.0),
                    target_lufs: Some(-16.0),
                    lufs_tolerance: Some(1.0),
                    require_audio_priming: true,
                    max_file_size_bytes: None,
                }),
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
                contract: Some(DeliveryContract::default()),
            },
        ];

        let mut watch_folder_engine = mediacraft_daemon::watch_folder::WatchFolderEngine::new();
        watch_folder_engine.add_rule(mediacraft_daemon::watch_folder::WatchFolderRule::default());

        let open_modal_initially =
            std::env::args().any(|a| a == "--modal" || a == "--export-settings");
        let initial_editing_preset = if open_modal_initially {
            presets.first().cloned()
        } else {
            None
        };

        let open_preferences_initially = std::env::args().any(|a| a == "--preferences");

        let initial_workspace = if std::env::args().any(|a| a == "--queue" || a == "--empty-queue")
        {
            WorkspaceMode::Queue
        } else if std::env::args().any(|a| a == "--automations") {
            WorkspaceMode::Automations
        } else if std::env::args().any(|a| a.starts_with("--deliver")) {
            WorkspaceMode::Deliver
        } else if std::env::args().any(|a| a == "--presets" || a == "--preset-diff") {
            WorkspaceMode::Presets
        } else if std::env::args().any(|a| a == "--process") {
            WorkspaceMode::Process
        } else if std::env::args().any(|a| a == "--inspect") {
            WorkspaceMode::Inspect
        } else if std::env::args().any(|a| a == "--collections") {
            WorkspaceMode::Collections
        } else {
            WorkspaceMode::Browse
        };

        if std::env::args().any(|a| a == "--empty-queue") {
            engine_arc
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear_all();
        }

        let mut preset_state = PresetViewState::default();
        if std::env::args().any(|a| a == "--preset-diff") {
            preset_state.active_tab = PresetWorkspaceTab::Comparison;
        }

        let mut deliver_state = DeliverViewState::default();
        if std::env::args().any(|a| a == "--deliver-validation") {
            deliver_state.active_tab = deliver_view::DeliverTab::Validation;
        } else if std::env::args().any(|a| a == "--deliver-quality") {
            deliver_state.active_tab = deliver_view::DeliverTab::QualityAnalysis;
        } else if std::env::args().any(|a| a == "--deliver-destinations") {
            deliver_state.active_tab = deliver_view::DeliverTab::Destinations;
        } else if std::env::args().any(|a| a == "--deliver-report-modal") {
            deliver_state.active_tab = deliver_view::DeliverTab::Validation;
            deliver_state.show_export_report_modal = true;
        }

        let mut inspector_state = InspectorState::default();
        if std::env::args().any(|a| a == "--validation") {
            inspector_state.active_tab = inspector_view::InspectorTab::Validation;
        } else if std::env::args().any(|a| a == "--metadata") {
            inspector_state.active_tab = inspector_view::InspectorTab::Metadata;
        }
        if std::env::args().any(|a| a == "--split-wipe") {
            inspector_state.preview_mode = inspector_view::PreviewMode::ComparisonSplit;
            inspector_state.split_pos = 0.52;
        }

        let mut library_state = LibraryViewState::default();
        if std::env::args().any(|a| a == "--batch-rename") {
            library_state.selected_ids.insert("A001_C001".to_string());
            library_state.selected_ids.insert("Drone_001".to_string());
            library_state.batch_rename_open = true;
        }
        if std::env::args().any(|a| a == "--ingest-modal") {
            library_state.scan_dialog_open = true;
        }
        if std::env::args().any(|a| a == "--relink-modal") {
            library_state.relink_target_asset_id = Some("Offline_Take_04".to_string());
            library_state.relink_modal_open = true;
        }
        if std::env::args().any(|a| a == "--contact-sheet-modal") {
            library_state.contact_sheet_modal_open = true;
        }

        Self {
            engine: engine_arc,
            ipc_server: Some(ipc_server),
            watch_folder_engine,
            catalog,
            selected_asset_id: Some("A001_C001".to_string()),
            search_query: String::new(),
            collection_manager: mediacraft_catalog::CollectionManager::with_defaults(),
            workspace_mode: initial_workspace,
            library_state,
            inspector_state,
            process_state: ProcessViewState::default(),
            sidebar_state: SidebarNavState::default(),
            drawer_state: QueueDrawerState::default(),
            queue_state: QueueViewState::default(),
            automate_state: AutomateViewState::default(),
            deliver_state,
            preset_state,
            layout,
            presets,
            editing_preset: initial_editing_preset,
            editing_tab: 0,
            app_icon,
            settings_store,
            preferences_open: open_preferences_initially,
            show_quit_confirm: false,
            quit_confirmed: false,
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

    fn handle_library_action(&mut self, action: Option<library_view::LibraryAction>) {
        let action = match action {
            Some(a) => a,
            None => return,
        };

        match action {
            library_view::LibraryAction::ScanDirectory(path) => {
                let count = self
                    .catalog
                    .scan_and_merge(&path, &mediacraft_catalog::scanner::ScanOptions::default());
                self.library_state.status_notification = Some(format!(
                    "Ingest complete: scanned {} asset(s) from {}",
                    count,
                    path.display()
                ));
            }
            library_view::LibraryAction::BatchRename {
                pattern,
                start_index,
            } => {
                let planner =
                    mediacraft_catalog::batch_rename::BatchRenamePlanner::new(pattern, start_index);
                let selected_assets: Vec<_> = self
                    .catalog
                    .all_assets()
                    .iter()
                    .filter(|a| self.library_state.selected_ids.contains(&a.id))
                    .cloned()
                    .collect();

                let plan = planner.plan(&selected_assets);
                match self.library_state.rollback_journal.execute(&plan) {
                    Ok(count) => {
                        for item in &plan {
                            if let Some(asset) = self
                                .catalog
                                .assets_mut()
                                .iter_mut()
                                .find(|a| a.id == item.asset_id)
                            {
                                asset.path = item.new_path.clone();
                                asset.filename = item.new_filename.clone();
                            }
                        }
                        self.library_state.status_notification = Some(format!(
                            "Batch rename succeeded: {} files renamed safely",
                            count
                        ));
                    }
                    Err(err) => {
                        self.library_state.status_notification =
                            Some(format!("Rename error: {err}"));
                    }
                }
            }
            library_view::LibraryAction::BatchRollback => {
                match self.library_state.rollback_journal.rollback_all() {
                    Ok(count) => {
                        for asset in self.catalog.assets_mut() {
                            if let Some(name) = asset.path.file_name().and_then(|n| n.to_str()) {
                                asset.filename = name.to_string();
                            }
                        }
                        self.library_state.status_notification = Some(format!(
                            "Rollback successful: {} files reverted to previous names",
                            count
                        ));
                    }
                    Err(err) => {
                        self.library_state.status_notification =
                            Some(format!("Rollback error: {err}"));
                    }
                }
            }
            library_view::LibraryAction::BatchTag { tag } => {
                let mutation = mediacraft_catalog::batch_rename::BatchMetadataMutation {
                    add_tags: vec![tag.clone()],
                    ..Default::default()
                };
                let selected = &self.library_state.selected_ids;
                let mut mutated_count = 0;
                for asset in self.catalog.assets_mut() {
                    if selected.contains(&asset.id) {
                        mutation.apply(std::slice::from_mut(asset));
                        mutated_count += 1;
                    }
                }
                self.library_state.status_notification =
                    Some(format!("Tagged {} assets with #{tag}", mutated_count));
            }
            library_view::LibraryAction::BatchRate { rating } => {
                let mutation = mediacraft_catalog::batch_rename::BatchMetadataMutation {
                    set_rating: Some(rating),
                    ..Default::default()
                };
                let selected = &self.library_state.selected_ids;
                let mut mutated_count = 0;
                for asset in self.catalog.assets_mut() {
                    if selected.contains(&asset.id) {
                        mutation.apply(std::slice::from_mut(asset));
                        mutated_count += 1;
                    }
                }
                self.library_state.status_notification = Some(format!(
                    "Updated {} assets to {} stars",
                    mutated_count, rating
                ));
            }
            library_view::LibraryAction::RelinkAsset { asset_id, new_path } => {
                if let Some(asset) = self
                    .catalog
                    .assets_mut()
                    .iter_mut()
                    .find(|a| a.id == asset_id)
                {
                    asset.path = new_path.clone();
                    if let Some(name) = new_path.file_name().and_then(|n| n.to_str()) {
                        asset.filename = name.to_string();
                    }
                    asset.is_offline = false;
                    self.library_state.status_notification = Some(format!(
                        "Relinked {} to online storage: {}",
                        asset.filename,
                        new_path.display()
                    ));
                }
            }
            library_view::LibraryAction::ExportContactSheet {
                project_name,
                columns,
            } => {
                let options = mediacraft_catalog::contact_sheet::ContactSheetOptions {
                    title: "MediaCraft Asset Contact Sheet".to_string(),
                    project_name,
                    include_technical_metadata: true,
                    include_tags: true,
                    columns,
                };
                let assets: Vec<_> = self.catalog.all_assets().to_vec();
                let html = mediacraft_catalog::contact_sheet::ContactSheetGenerator::generate_html(
                    &assets, &options,
                );
                let out_path = std::env::temp_dir().join("mediacraft_contact_sheet.html");
                let _ = std::fs::write(&out_path, &html);
                self.library_state.status_notification = Some(format!(
                    "Exported Contact Sheet ({} assets) to {}",
                    assets.len(),
                    out_path.display()
                ));
            }
        }
    }

    fn advance_simulation(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f32();
        self.last_tick = now;

        // Poll engine progress
        let engine_arc = Arc::clone(&self.engine);
        let mut eng = engine_arc.lock().unwrap_or_else(PoisonError::into_inner);

        // Scan watch folders if monitoring is active
        if self.automate_state.is_monitoring_active {
            let _ = self
                .watch_folder_engine
                .scan_and_enqueue(&mut eng, &self.presets);
        }

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
                    let _ = eng.execute_job_to_completion(job.id);
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

                        let tab_labels = ["Video", "Audio", "Multiplexer", "Effects"];
                        let tab_fit = craft_ui::tabs::fit(
                            &[65.0, 65.0, 95.0, 75.0],
                            self.editing_tab,
                            ui.available_width(),
                            45.0,
                            20.0,
                        );
                        ui.horizontal(|ui| {
                            for (idx, width) in tab_fit.shown {
                                if let Some(label) = tab_labels.get(idx) {
                                    let is_selected = self.editing_tab == idx;
                                    let (rect, _) = ui.allocate_exact_size(
                                        Vec2::new(width, 22.0),
                                        egui::Sense::hover(),
                                    );
                                    let tab_widget = craft_ui::tabs::Tab::new(
                                        ui.id().with(("export_settings_tab", idx)),
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
                                            let underline = Rect::from_min_max(
                                                Pos2::new(rect.min.x, rect.max.y - 2.0),
                                                rect.max,
                                            );
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
                                        self.editing_tab = idx;
                                    }
                                }
                            }
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
                                ui.label("• Creative Look / 3D LUT: None");
                                ui.label("• Burn-in Timecode: Disabled");
                            }
                        }
                    });
                });

                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let normal_style = craft_theme::craft_icon_button_style();
                        let accent_style = craft_theme::craft_accent_button_style();
                        let danger_style = craft_theme::craft_danger_button_style();

                        if craft_ui::buttons::IconButton::new(
                            "Cancel",
                            Vec2::new(75.0, 24.0),
                            &danger_style,
                        )
                        .show(ui, |ui, rect, tint| {
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "Cancel",
                                egui::FontId::proportional(11.0),
                                tint,
                            );
                        })
                        .clicked()
                        {
                            close_modal = true;
                        }

                        ui.add_space(6.0);
                        if craft_ui::buttons::IconButton::new(
                            "Queue",
                            Vec2::new(75.0, 24.0),
                            &accent_style,
                        )
                        .show(ui, |ui, rect, tint| {
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "Queue",
                                egui::FontId::proportional(11.0),
                                tint,
                            );
                        })
                        .clicked()
                        {
                            enqueue_modal = true;
                        }

                        ui.add_space(6.0);
                        if craft_ui::buttons::IconButton::new(
                            "Save & Close",
                            Vec2::new(100.0, 24.0),
                            &normal_style,
                        )
                        .show(ui, |ui, rect, tint| {
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "Save & Close",
                                egui::FontId::proportional(11.0),
                                tint,
                            );
                        })
                        .clicked()
                        {
                            save_modal = true;
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

    fn show_preferences_modal(&mut self, ctx: &egui::Context) {
        if !self.preferences_open {
            return;
        }

        let mut close = false;
        let mut current_prefs = self.settings_store.get().clone();
        let mut changed = false;

        egui::Window::new("MediaCraft Preferences")
            .fixed_size(Vec2::new(560.0, 360.0))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Application & Pipeline Settings (Managed via craft-settings)")
                        .size(11.0)
                        .color(Color32::GRAY),
                );
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Default Output Directory:").strong().size(11.5));
                    ui.add(
                        egui::TextEdit::singleline(&mut current_prefs.default_output_folder)
                            .desired_width(280.0),
                    );
                });
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.label("Target Video Bitrate (kbps):");
                    let mut br = current_prefs.target_bitrate_kbps;
                    if ui
                        .add(egui::Slider::new(&mut br, 2_000..=150_000).suffix(" kbps"))
                        .changed()
                    {
                        current_prefs.target_bitrate_kbps = br;
                        changed = true;
                    }
                });
                ui.add_space(8.0);

                if ui
                    .checkbox(
                        &mut current_prefs.faststart_enabled,
                        "Enable FastStart ISO BMFF layout (moov before mdat)",
                    )
                    .changed()
                {
                    changed = true;
                }
                ui.label(
                    RichText::new(
                        "Enables progressive streaming and immediate playback on web and broadcast servers.",
                    )
                    .size(10.0)
                    .color(Color32::GRAY),
                );
                ui.add_space(6.0);

                if ui
                    .checkbox(
                        &mut current_prefs.audio_priming_enabled,
                        "Sample-accurate AAC priming delay compensation (1024 samples)",
                    )
                    .changed()
                {
                    changed = true;
                }
                ui.label(
                    RichText::new(
                        "Trim initial encoder delay frames via edit list (elst) atom to ensure AV sync.",
                    )
                    .size(10.0)
                    .color(Color32::GRAY),
                );
                ui.add_space(6.0);

                if ui
                    .checkbox(
                        &mut current_prefs.auto_pause_on_playback,
                        "Auto-pause queue when external NLE playback is active (craft-daemon hook)",
                    )
                    .changed()
                {
                    changed = true;
                }
                ui.label(
                    RichText::new(
                        "Automatically suspends background encodes to free GPU/CPU bandwidth during live editing.",
                    )
                    .size(10.0)
                    .color(Color32::GRAY),
                );
                ui.add_space(16.0);
                ui.separator();

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn_style = craft_theme::craft_icon_button_style();
                        if craft_ui::buttons::IconButton::new(
                            "Done",
                            Vec2::new(80.0, 24.0),
                            &btn_style,
                        )
                        .show(ui, |ui, rect, tint| {
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "Done",
                                egui::FontId::proportional(11.0),
                                tint,
                            );
                        })
                        .clicked()
                        {
                            close = true;
                        }
                    });
                });
            });

        if changed {
            self.settings_store.update(|prefs| {
                *prefs = current_prefs;
            });
        }

        if close {
            self.preferences_open = false;
        }
    }
}

impl eframe::App for MediaCraftApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.advance_simulation();
        self.settings_store.tick(Instant::now());

        let has_active_encoding = self.engine_lock().active_count() > 0;
        if ctx.input(|i| i.viewport().close_requested())
            && has_active_encoding
            && !self.quit_confirmed
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show_quit_confirm = true;
        }

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

        // Global hotkey: Cmd+, opens Preferences
        if ui
            .input(|i| (i.modifiers.command || i.modifiers.ctrl) && i.key_pressed(egui::Key::Comma))
        {
            self.preferences_open = !self.preferences_open;
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

            // Workspace Mode Switcher via craft-ui Tab strip
            let tabs = [
                ("⊞ Browse", WorkspaceMode::Browse),
                ("🔍 Inspect", WorkspaceMode::Inspect),
                ("📁 Collections", WorkspaceMode::Collections),
                ("⚡ Process", WorkspaceMode::Process),
                ("📋 Queue", WorkspaceMode::Queue),
                ("⚙ Automations", WorkspaceMode::Automations),
                ("📦 Deliver", WorkspaceMode::Deliver),
                ("🎛 Presets", WorkspaceMode::Presets),
            ];
            let active_idx = tabs
                .iter()
                .position(|(_, m)| *m == self.workspace_mode)
                .unwrap_or(0);
            let tab_fit = craft_ui::tabs::fit(
                &[82.0, 84.0, 104.0, 84.0, 82.0, 112.0, 86.0, 88.0],
                active_idx,
                ui.available_width().min(760.0),
                50.0,
                20.0,
            );
            for (idx, width) in tab_fit.shown {
                if let Some((label, mode)) = tabs.get(idx) {
                    let is_selected = active_idx == idx;
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(width, 24.0), egui::Sense::hover());
                    let tab_widget = craft_ui::tabs::Tab::new(
                        ui.id().with(("workspace_nav_tab", idx)),
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
                            let underline = Rect::from_min_max(
                                Pos2::new(rect.min.x, rect.max.y - 2.0),
                                rect.max,
                            );
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
                            egui::FontId::proportional(11.5),
                            text_color,
                        );
                    });
                    if response.clicked() {
                        self.workspace_mode = *mode;
                    }
                }
            }

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

                ui.add_space(4.0);
                if craft_ui::buttons::IconButton::new(
                    "Preferences",
                    Vec2::new(105.0, 24.0),
                    &btn_style,
                )
                .selected(self.preferences_open)
                .show(ui, |ui, rect, tint| {
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "⚙ Settings (⌘,)",
                        egui::FontId::proportional(11.0),
                        tint,
                    );
                })
                .clicked()
                {
                    self.preferences_open = !self.preferences_open;
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
                let engine_arc = Arc::clone(&self.engine);
                let mut queue_lock = engine_arc.lock().unwrap_or_else(PoisonError::into_inner);
                render_automate_workspace(
                    ui,
                    &mut self.automate_state,
                    &mut self.watch_folder_engine,
                    &mut queue_lock,
                    &self.presets,
                );
            }
            WorkspaceMode::Deliver => {
                render_deliver_workspace(ui, &mut self.deliver_state);
            }
            WorkspaceMode::Presets => {
                render_preset_workspace(ui, &mut self.preset_state, &mut self.presets);
            }
            WorkspaceMode::Inspect => {
                let selected_asset = self
                    .selected_asset_id
                    .as_deref()
                    .and_then(|id| self.catalog.get_by_id(id));

                render_inspector_panel(
                    ui,
                    &mut self.inspector_state,
                    selected_asset,
                    self.catalog.graph(),
                );

                ui.separator();
                render_queue_drawer(ui, &mut self.drawer_state, &self.engine);
            }
            WorkspaceMode::Collections => {
                let counts = self.catalog.kind_counts();
                let total = self.catalog.all_assets().len();
                let (collection_label, filtered_assets): (String, Vec<&MediaAsset>) =
                    if let Some(ref custom_id) = self.sidebar_state.selected_custom_collection_id {
                        let label = self
                            .sidebar_state
                            .selected_custom_label
                            .clone()
                            .unwrap_or_else(|| custom_id.clone());
                        let mut assets = if custom_id.starts_with("smart_") {
                            self.collection_manager
                                .filter_smart(custom_id, self.catalog.all())
                        } else {
                            self.collection_manager
                                .filter_static(custom_id, self.catalog.all())
                        };
                        if let Some(kind) = self.sidebar_state.selected_kind_filter {
                            assets.retain(|a| a.kind == kind);
                        }
                        let q = self.search_query.trim().to_lowercase();
                        if !q.is_empty() {
                            assets.retain(|a| {
                                a.filename.to_lowercase().contains(&q)
                                    || a.id.to_lowercase().contains(&q)
                            });
                        }
                        (label, assets)
                    } else {
                        let label = self.sidebar_state.selected_collection.label().to_string();
                        let assets = self.catalog.query(
                            self.sidebar_state.selected_collection,
                            &self.search_query,
                            self.sidebar_state.selected_kind_filter,
                        );
                        (label, assets)
                    };

                let mut library_action = None;
                ui.columns(2, |columns| {
                    render_sidebar_nav(&mut columns[0], &mut self.sidebar_state, &counts, total);
                    library_action = render_library_view(
                        &mut columns[1],
                        &mut self.library_state,
                        LibraryViewContext {
                            active_collection: self.sidebar_state.selected_collection,
                            collection_label: &collection_label,
                            kind_filter: &mut self.sidebar_state.selected_kind_filter,
                            kind_counts: &counts,
                            total_assets: total,
                            search_query: &mut self.search_query,
                            assets: &filtered_assets,
                            selected_asset_id: &mut self.selected_asset_id,
                            stacks: self.catalog.stacks(),
                        },
                    );
                });
                self.handle_library_action(library_action);

                ui.separator();
                render_queue_drawer(ui, &mut self.drawer_state, &self.engine);
            }
            WorkspaceMode::Process => {
                let selected_asset = self
                    .selected_asset_id
                    .as_deref()
                    .and_then(|id| self.catalog.get_by_id(id));
                let mut enqueue_job_action: Option<(EncodingPreset, String, String)> = None;

                ui.columns(2, |columns| {
                    render_inspector_panel(
                        &mut columns[0],
                        &mut self.inspector_state,
                        selected_asset,
                        self.catalog.graph(),
                    );
                    render_process_panel(
                        &mut columns[1],
                        &mut self.process_state,
                        selected_asset,
                        &self.presets,
                        &mut enqueue_job_action,
                    );
                });

                if let Some((preset, src, out)) = enqueue_job_action {
                    let _ = self.engine_lock().add_job(
                        PathBuf::from(src),
                        PathBuf::from(out),
                        preset,
                        5,
                    );
                    self.drawer_state.is_expanded = true;
                }

                ui.separator();
                render_queue_drawer(ui, &mut self.drawer_state, &self.engine);
            }
            WorkspaceMode::Browse => {
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

                let (collection_label, filtered_assets): (String, Vec<&MediaAsset>) =
                    if let Some(ref custom_id) = self.sidebar_state.selected_custom_collection_id {
                        let label = self
                            .sidebar_state
                            .selected_custom_label
                            .clone()
                            .unwrap_or_else(|| custom_id.clone());
                        let mut assets = if custom_id.starts_with("smart_") {
                            self.collection_manager
                                .filter_smart(custom_id, self.catalog.all())
                        } else {
                            self.collection_manager
                                .filter_static(custom_id, self.catalog.all())
                        };
                        if let Some(kind) = self.sidebar_state.selected_kind_filter {
                            assets.retain(|a| a.kind == kind);
                        }
                        let q = self.search_query.trim().to_lowercase();
                        if !q.is_empty() {
                            assets.retain(|a| {
                                a.filename.to_lowercase().contains(&q)
                                    || a.id.to_lowercase().contains(&q)
                            });
                        }
                        (label, assets)
                    } else {
                        let label = self.sidebar_state.selected_collection.label().to_string();
                        let assets = self.catalog.query(
                            self.sidebar_state.selected_collection,
                            &self.search_query,
                            self.sidebar_state.selected_kind_filter,
                        );
                        (label, assets)
                    };

                let selected_asset = self
                    .selected_asset_id
                    .as_deref()
                    .and_then(|id| self.catalog.get_by_id(id));

                let total = self.catalog.all_assets().len();
                let counts = self.catalog.kind_counts();
                let mut enqueue_job_action: Option<(EncodingPreset, String, String)> = None;
                let mut library_action = None;

                let output = DockArea::new(egui::Id::new("mediacraft_workspace_dock")).show(
                    ui,
                    &layout,
                    &style,
                    |panel| panel.label().into(),
                    |_| Permissions::default(),
                    |ui, panel| match *panel {
                        Panel::Sidebar => {
                            render_sidebar_nav(ui, &mut self.sidebar_state, &counts, total);
                        }
                        Panel::LibraryGrid => {
                            library_action = render_library_view(
                                ui,
                                &mut self.library_state,
                                LibraryViewContext {
                                    active_collection: self.sidebar_state.selected_collection,
                                    collection_label: &collection_label,
                                    kind_filter: &mut self.sidebar_state.selected_kind_filter,
                                    kind_counts: &counts,
                                    total_assets: total,
                                    search_query: &mut self.search_query,
                                    assets: &filtered_assets,
                                    selected_asset_id: &mut self.selected_asset_id,
                                    stacks: self.catalog.stacks(),
                                },
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
                self.handle_library_action(library_action);

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

        // 5. Preferences Modal (via craft-settings)
        self.show_preferences_modal(ui.ctx());

        // 6. Safe Quit Confirmation Dialog (via craft-quit)
        if self.show_quit_confirm {
            egui::Window::new("Transcoding in Progress")
                .fixed_size(Vec2::new(440.0, 160.0))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ui.ctx(), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("⚠️ Active Encoding Jobs in Queue")
                                .strong()
                                .size(13.0)
                                .color(Color32::from_rgb(255, 190, 70)),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            "Exiting MediaCraft now will cancel actively running transcodes. Do you wish to cancel all active jobs and quit?",
                        );
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            ui.add_space((ui.available_width() - 220.0) * 0.5);
                            let btn_style = craft_theme::craft_icon_button_style();
                            let danger_style = craft_theme::craft_danger_button_style();
                            if craft_ui::buttons::IconButton::new(
                                "Cancel",
                                Vec2::new(90.0, 26.0),
                                &btn_style,
                            )
                            .show(ui, |ui, rect, tint| {
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "Cancel",
                                    egui::FontId::proportional(11.0),
                                    tint,
                                );
                            })
                            .clicked()
                            {
                                self.show_quit_confirm = false;
                            }
                            ui.add_space(8.0);
                            if craft_ui::buttons::IconButton::new(
                                "Quit Anyway",
                                Vec2::new(110.0, 26.0),
                                &danger_style,
                            )
                            .show(ui, |ui, rect, tint| {
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "Quit Anyway",
                                    egui::FontId::proportional(11.0),
                                    tint,
                                );
                            })
                            .clicked()
                            {
                                self.quit_confirmed = true;
                                self.show_quit_confirm = false;
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    });
                });
        }
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
