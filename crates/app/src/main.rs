mod app_icon;
mod automate_view;
pub mod craft_theme;
mod inspector_view;
mod library_view;
mod mac_menu;
mod media_preview;
mod process_view;
mod queue_drawer;
mod queue_view;
pub mod receipts;
pub mod settings;
mod sidebar_nav;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use egui::{Color32, CornerRadius, FontId, Pos2, Rect, RichText, Stroke, Vec2};

use craft_ui::docking::{DockArea, DockStyle, Layout, Node, Permissions};
use craft_ui::layout::{SplitAxis, SplitSize};

use mediacraft_catalog::{MediaAsset, MediaCatalog};
use mediacraft_daemon::QueueEngine;
use mediacraft_daemon::ipc::IpcServer;
use mediacraft_protocol::{
    AudioCodec, ContainerFormat, DeliveryContract, EncodingPreset, VideoCodec,
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
    LibraryGrid,
    Inspector,
}

impl Panel {
    pub fn label(&self) -> &'static str {
        match self {
            Self::LibraryGrid => "Media Library",
            Self::Inspector => "Preview & Inspector",
        }
    }
}

pub struct MediaCraftApp {
    engine: Arc<Mutex<QueueEngine>>,
    ipc_server: Option<IpcServer>,
    watch_folder_engine: mediacraft_daemon::watch_folder::WatchFolderEngine,
    catalog: MediaCatalog,
    scan_result_rx: Option<
        Receiver<(
            PathBuf,
            Vec<MediaAsset>,
            mediacraft_catalog::scanner::ScanReport,
        )>,
    >,
    selected_asset_id: Option<String>,
    search_query: String,
    collection_manager: mediacraft_catalog::CollectionManager,

    // Sub-view states
    workspace_mode: WorkspaceMode,
    previews: media_preview::MediaPreviewCache,
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
    preferences_tab: usize,

    // Safe termination lifecycle via craft-quit
    show_quit_confirm: bool,
    quit_confirmed: bool,

    mac_menu: Option<mac_menu::MacMenu>,

    last_tick: Instant,
}

impl MediaCraftApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        craft_theme::apply_global_theme(&cc.egui_ctx);
        let mac_menu = mac_menu::MacMenu::install(&cc.egui_ctx);
        let app_icon = cc.egui_ctx.load_texture(
            "mediacraft_app_icon",
            app_icon::app_icon_image(),
            egui::TextureOptions::LINEAR,
        );
        let settings_store = settings::open_settings();
        let engine = QueueEngine::new(2);

        let engine_arc = Arc::new(Mutex::new(engine));
        let ipc_server = IpcServer::new("/tmp/mediacraft.sock", Arc::clone(&engine_arc));
        let _ = ipc_server.start();

        let catalog = MediaCatalog::new();

        // Keep the browse workspace focused on the media grid and its inspector.
        let root_node = Node::Split {
            axis: SplitAxis::Horizontal,
            size: SplitSize::FixedSecond(420.0),
            first: Box::new(Node::Tabs {
                panels: vec![Panel::LibraryGrid],
                active: 0,
            }),
            second: Box::new(Node::Tabs {
                panels: vec![Panel::Inspector],
                active: 0,
            }),
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

        let open_preferences_initially =
            std::env::args().any(|a| a == "--preferences" || a == "--hardware-settings");
        let initial_preferences_tab = if std::env::args().any(|a| a == "--hardware-settings") {
            1
        } else {
            0
        };

        let initial_workspace = if std::env::args().any(|a| {
            a == "--queue"
                || a == "--empty-queue"
                || a == "--error-modal"
                || a == "--unsupported-modal"
                || a == "--batch-edit-modal"
                || a == "--completion-modal"
        }) {
            WorkspaceMode::Queue
        } else if std::env::args()
            .any(|a| a == "--automations" || a == "--workflow-builder" || a == "--ingest-schedules")
        {
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

        let mut queue_state = QueueViewState::default();
        if std::env::args().any(|a| a == "--error-modal") {
            queue_state.show_error_modal = true;
        } else if std::env::args().any(|a| a == "--unsupported-modal") {
            queue_state.show_unsupported_modal = true;
        } else if std::env::args().any(|a| a == "--batch-edit-modal") {
            queue_state.show_batch_edit_modal = true;
            let jobs = engine_arc
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .list_jobs();
            for j in jobs.iter().take(3) {
                queue_state.selected_jobs.insert(j.id);
            }
        } else if std::env::args().any(|a| a == "--completion-modal") {
            queue_state.show_completion_modal = true;
        }

        let mut automate_state = AutomateViewState::default();
        if std::env::args().any(|a| a == "--workflow-builder") {
            automate_state.active_tab = automate_view::AutomateTab::WorkflowPipelines;
        } else if std::env::args().any(|a| a == "--ingest-schedules") {
            automate_state.active_tab = automate_view::AutomateTab::IngestSchedules;
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
            library_state.batch_rename_open = true;
        }
        if std::env::args().any(|a| a == "--ingest-modal") {
            library_state.scan_dialog_open = true;
        }
        if std::env::args().any(|a| a == "--relink-modal") {
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
            scan_result_rx: None,
            selected_asset_id: None,
            search_query: String::new(),
            collection_manager: mediacraft_catalog::CollectionManager::with_defaults(),
            workspace_mode: initial_workspace,
            previews: media_preview::MediaPreviewCache::default(),
            library_state,
            inspector_state,
            process_state: ProcessViewState::default(),
            sidebar_state: SidebarNavState::default(),
            drawer_state: QueueDrawerState::default(),
            queue_state,
            automate_state,
            deliver_state,
            preset_state,
            layout,
            presets,
            editing_preset: initial_editing_preset,
            editing_tab: 0,
            app_icon,
            settings_store,
            preferences_open: open_preferences_initially,
            preferences_tab: initial_preferences_tab,
            show_quit_confirm: false,
            quit_confirmed: false,
            mac_menu,
            last_tick: Instant::now(),
        }
    }

    fn engine_lock(&self) -> std::sync::MutexGuard<'_, QueueEngine> {
        self.engine.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn apply_craft_theme(&self, ui: &mut egui::Ui) {
        craft_theme::apply_global_theme(ui.ctx());
    }

    fn handle_library_action(&mut self, action: Option<library_view::LibraryAction>) {
        let action = match action {
            Some(a) => a,
            None => return,
        };

        match action {
            library_view::LibraryAction::ScanDirectory(path) => {
                if self.scan_result_rx.is_some() {
                    self.library_state.status_notification =
                        Some("A folder scan is already running.".to_string());
                    return;
                }

                let (result_tx, result_rx) = mpsc::sync_channel(1);
                let scan_path = path.clone();
                let worker = std::thread::Builder::new()
                    .name("mediacraft-folder-scan".to_string())
                    .spawn(move || {
                        let (assets, report) = mediacraft_catalog::scanner::scan_path_with_report(
                            &scan_path,
                            &mediacraft_catalog::scanner::ScanOptions::default(),
                        );
                        let _ = result_tx.send((scan_path, assets, report));
                    });
                match worker {
                    Ok(_) => {
                        self.scan_result_rx = Some(result_rx);
                        self.library_state.status_notification =
                            Some(format!("Scanning {}…", path.display()));
                    }
                    Err(error) => {
                        self.library_state.status_notification =
                            Some(format!("Could not start folder scan: {error}"));
                    }
                }
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
        self.poll_folder_scan();

        if self.inspector_state.is_playing {
            let duration = self
                .selected_asset_id
                .as_deref()
                .and_then(|id| self.catalog.get_by_id(id))
                .filter(|asset| asset.kind == mediacraft_catalog::MediaKind::Video)
                .and_then(|asset| asset.duration_secs)
                .filter(|duration| *duration > 0.0);
            if let Some(duration) = duration {
                self.inspector_state.scrub_pos += dt / duration;
                if self.inspector_state.scrub_pos >= 1.0 {
                    self.inspector_state.scrub_pos = 0.0;
                }
            } else {
                self.inspector_state.is_playing = false;
            }
        }

        // Poll engine progress
        let engine_arc = Arc::clone(&self.engine);
        let mut eng = engine_arc.lock().unwrap_or_else(PoisonError::into_inner);

        // Scan watch folders if monitoring is active
        if self.automate_state.is_monitoring_active {
            let _ = self
                .watch_folder_engine
                .scan_and_enqueue(&mut eng, &self.presets);
        }
    }

    fn poll_folder_scan(&mut self) {
        let result = match self.scan_result_rx.as_ref() {
            Some(receiver) => match receiver.try_recv() {
                Ok(result) => Some(Ok(result)),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => Some(Err(())),
            },
            None => return,
        };
        self.scan_result_rx = None;

        match result {
            Some(Ok((path, mut assets, report))) => {
                assets.sort_by(|left, right| {
                    left.filename
                        .to_ascii_lowercase()
                        .cmp(&right.filename.to_ascii_lowercase())
                        .then_with(|| left.path.cmp(&right.path))
                });
                let mut indexed_paths: HashSet<PathBuf> = self
                    .catalog
                    .all_assets()
                    .iter()
                    .map(|asset| asset.path.clone())
                    .collect();
                let mut added = 0;
                for asset in assets {
                    if indexed_paths.insert(asset.path.clone()) {
                        self.catalog.add_asset(asset);
                        added += 1;
                    }
                }
                if self.selected_asset_id.is_none() {
                    self.selected_asset_id = self
                        .catalog
                        .all_assets()
                        .first()
                        .map(|asset| asset.id.clone());
                }
                self.library_state.status_notification = Some(format!(
                    "Ingested {added} file(s) from {} • {} unsupported • {} scan errors",
                    path.display(),
                    report.skipped_unsupported,
                    report.errors.len()
                ));
            }
            Some(Err(())) => {
                self.library_state.status_notification =
                    Some("Folder scan stopped before it could return results.".to_string());
            }
            None => {}
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
        let selected_asset = self
            .selected_asset_id
            .as_deref()
            .and_then(|id| self.catalog.get_by_id(id));

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
                        if let Some(asset) = selected_asset {
                            media_preview::paint_asset_preview(
                                &mut self.previews,
                                ui.ctx(),
                                &painter,
                                rect,
                                asset,
                                0.5,
                            );
                        } else {
                            painter.rect_filled(
                                rect,
                                CornerRadius::same(3),
                                Color32::from_rgb(15, 18, 22),
                            );
                            painter.text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "Select a source file in Browse",
                                FontId::proportional(11.0),
                                Color32::GRAY,
                            );
                        }

                        ui.add_space(4.0);
                        if let Some(asset) = selected_asset {
                            ui.label(RichText::new(&asset.filename).strong().size(11.0));
                            ui.label(
                                RichText::new(format!(
                                    "{} • {} • {}",
                                    asset.kind.label(),
                                    asset.formatted_duration(),
                                    asset.formatted_size()
                                ))
                                .size(10.0)
                                .color(Color32::GRAY),
                            );
                            if let Some(video) = &asset.video {
                                ui.label(format!(
                                    "{} × {} • {}",
                                    video.width,
                                    video.height,
                                    if video.fps > 0.0 {
                                        format!("{:.3} fps", video.fps)
                                    } else {
                                        "frame rate unknown".to_string()
                                    }
                                ));
                            }
                        }
                        if let (Some(duration), Some(bitrate)) = (
                            selected_asset.and_then(|asset| asset.duration_secs),
                            preset.video_bitrate_kbps,
                        ) {
                            let estimated_mb =
                                duration as f64 * (bitrate as f64 * 1000.0) / 8.0 / 1_000_000.0;
                            ui.label(
                                RichText::new(format!(
                                    "Estimated video size: {estimated_mb:.1} MB"
                                ))
                                .size(10.5)
                                .color(Color32::from_rgb(80, 220, 120)),
                            );
                        }
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
                        ui.add_enabled_ui(selected_asset.is_some(), |ui| {
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
                        });

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
            if let Some(asset) = selected_asset {
                if !asset.path.is_file() {
                    self.library_state.status_notification =
                        Some("Source file is offline; reconnect it before queueing.".to_string());
                    self.editing_preset = Some(preset);
                    return;
                }

                let output_dir = self.settings_store.get().default_output_folder.as_str();
                let output_dir = if output_dir.is_empty() {
                    asset
                        .path
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new("."))
                } else {
                    std::path::Path::new(output_dir)
                };
                let extension = match preset.container {
                    ContainerFormat::Mp4 => "mp4",
                    ContainerFormat::Mov => "mov",
                    ContainerFormat::M4a => "m4a",
                    ContainerFormat::Wav => "wav",
                    ContainerFormat::Mxf => "mxf",
                };
                let stem = asset
                    .path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("output");
                let output = output_dir.join(format!("{stem}_{}.{}", preset.id, extension));

                if output.exists() {
                    self.library_state.status_notification = Some(format!(
                        "Output already exists; choose another output folder: {}",
                        output.display()
                    ));
                    self.editing_preset = Some(preset);
                    return;
                }

                self.engine_lock()
                    .add_job(asset.path.clone(), output, preset.clone(), 5);
                self.drawer_state.is_expanded = true;
            }
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
        let mut restore_defaults = false;

        egui::Window::new("MediaCraft Preferences (MD-12 / MD-13)")
            .fixed_size(Vec2::new(640.0, 440.0))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("⚙ Application & Engine Preferences")
                            .size(13.0)
                            .strong()
                            .color(Color32::from_rgb(240, 245, 255)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("craft-settings v1.0 • Atomic TOML")
                                .size(9.5)
                                .monospace()
                                .color(Color32::from_rgb(130, 145, 165)),
                        );
                    });
                });
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Configure filesystem paths, hardware accelerators, color pipelines, and daemon hooks.")
                        .size(10.0)
                        .color(Color32::GRAY),
                );
                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                // Preference Tabs
                let tabs = [
                    "General & Storage (MD-12)",
                    "Hardware & Encoders (MD-13)",
                    "Color & Processing",
                    "Audio & NLE Sync",
                ];
                ui.horizontal(|ui| {
                    for (idx, name) in tabs.iter().enumerate() {
                        let selected = self.preferences_tab == idx;
                        let text = if selected {
                            RichText::new(*name).size(11.0).strong().color(Color32::from_rgb(255, 180, 80))
                        } else {
                            RichText::new(*name).size(11.0).color(Color32::from_rgb(180, 195, 215))
                        };
                        if ui.selectable_label(selected, text).clicked() {
                            self.preferences_tab = idx;
                        }
                    }
                });
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(8.0);

                // Tab Contents
                match self.preferences_tab {
                    0 => {
                        // General & Storage (MD-12)
                        ui.label(RichText::new("FILESYSTEM PATHS & STORAGE").size(10.0).strong().color(Color32::from_rgb(140, 160, 185)));
                        ui.add_space(4.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Default Output Directory:").size(11.0));
                            if ui.add(egui::TextEdit::singleline(&mut current_prefs.default_output_folder).desired_width(340.0)).changed() {
                                changed = true;
                            }
                        });
                        ui.add_space(6.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Scratch Disk Cache Directory:").size(11.0));
                            if ui.add(egui::TextEdit::singleline(&mut current_prefs.scratch_disk_folder).desired_width(320.0)).changed() {
                                changed = true;
                            }
                        });
                        ui.label(
                            RichText::new("Fast local NVMe directory for intermediate transcode frames and demuxed elementary streams.")
                                .size(9.5)
                                .color(Color32::GRAY),
                        );
                        ui.add_space(10.0);

                        ui.label(RichText::new("CONTAINER PACKAGING & STREAMING").size(10.0).strong().color(Color32::from_rgb(140, 160, 185)));
                        ui.add_space(4.0);

                        if ui.checkbox(&mut current_prefs.faststart_enabled, "Enable FastStart ISO BMFF layout (moov atom before mdat)").changed() {
                            changed = true;
                        }
                        ui.label(
                            RichText::new("Enables progressive streaming and immediate Web/HLS playback without downloading the entire asset.")
                                .size(9.5)
                                .color(Color32::GRAY),
                        );
                        ui.add_space(8.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Default Catalog Thumbnail Zoom:").size(11.0));
                            if ui.add(egui::Slider::new(&mut current_prefs.thumbnail_zoom, 100.0..=280.0).suffix(" px")).changed() {
                                changed = true;
                            }
                        });
                    }
                    1 => {
                        // Hardware & Encoders (MD-13)
                        ui.label(RichText::new("HARDWARE ACCELERATION ENGINE").size(10.0).strong().color(Color32::from_rgb(140, 160, 185)));
                        ui.add_space(4.0);

                        let engines = [
                            "Apple VideoToolbox (Hardware)",
                            "Intel QuickSync Video (QSV)",
                            "NVIDIA NVENC / NVDEC",
                            "Software CPU (libx264 / rav1e / svt-av1)",
                        ];
                        for eng in engines {
                            let selected = current_prefs.hardware_engine == eng;
                            if ui.radio(selected, eng).clicked() {
                                current_prefs.hardware_engine = eng.to_string();
                                changed = true;
                            }
                        }
                        ui.add_space(6.0);

                        if ui.checkbox(&mut current_prefs.hardware_decode, "Enable hardware-accelerated decode for source media").changed() {
                            changed = true;
                        }
                        ui.label(
                            RichText::new("Uses dedicated GPU silicon for high-throughput 10-bit HEVC, ProRes, and H.264 source decoding.")
                                .size(9.5)
                                .color(Color32::GRAY),
                        );
                        ui.add_space(10.0);

                        ui.label(RichText::new("CONCURRENCY & MEMORY ARCHITECTURE").size(10.0).strong().color(Color32::from_rgb(140, 160, 185)));
                        ui.add_space(4.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Worker Thread Concurrency:").size(11.0));
                            let mut wt = current_prefs.worker_threads;
                            if ui.add(egui::Slider::new(&mut wt, 1..=32).suffix(" threads")).changed() {
                                current_prefs.worker_threads = wt;
                                changed = true;
                            }
                        });
                        ui.add_space(4.0);

                        if ui.checkbox(&mut current_prefs.zero_copy_buffers, "Zero-copy unified memory frame buffers (CVPixelBuffer / Metal)").changed() {
                            changed = true;
                        }
                        if ui.checkbox(&mut current_prefs.thermal_throttling, "Thermal-aware throttling (suspend queue under high SoC temperature)").changed() {
                            changed = true;
                        }
                    }
                    2 => {
                        // Color & Processing
                        ui.label(RichText::new("COLOR MANAGEMENT & WORKING SPACE").size(10.0).strong().color(Color32::from_rgb(140, 160, 185)));
                        ui.add_space(4.0);

                        let spaces = [
                            "Rec.709 (BT.709)",
                            "Display P3 (Wide Gamut)",
                            "Rec.2020 / UHD HDR",
                        ];
                        for sp in spaces {
                            let selected = current_prefs.default_color_space == sp;
                            if ui.radio(selected, sp).clicked() {
                                current_prefs.default_color_space = sp.to_string();
                                changed = true;
                            }
                        }
                        ui.add_space(10.0);

                        ui.label(RichText::new("HDR TONE MAPPING OPERATOR").size(10.0).strong().color(Color32::from_rgb(140, 160, 185)));
                        ui.add_space(4.0);

                        let ops = [
                            "Hable / Filmic Curve",
                            "Reinhard Global Operator",
                            "ITU-R BT.2446 Method A",
                        ];
                        for op in ops {
                            let selected = current_prefs.tone_mapping_operator == op;
                            if ui.radio(selected, op).clicked() {
                                current_prefs.tone_mapping_operator = op.to_string();
                                changed = true;
                            }
                        }
                        ui.add_space(10.0);

                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Default Master Bitrate:").size(11.0));
                            let mut br = current_prefs.target_bitrate_kbps;
                            if ui.add(egui::Slider::new(&mut br, 2_000..=150_000).suffix(" kbps")).changed() {
                                current_prefs.target_bitrate_kbps = br;
                                changed = true;
                            }
                        });
                    }
                    _ => {
                        // Audio & NLE Sync
                        ui.label(RichText::new("SAMPLE ACCURACY & AV SYNCHRONIZATION").size(10.0).strong().color(Color32::from_rgb(140, 160, 185)));
                        ui.add_space(4.0);

                        if ui.checkbox(&mut current_prefs.audio_priming_enabled, "Sample-accurate AAC priming delay compensation (1024 samples elst trim)").changed() {
                            changed = true;
                        }
                        ui.label(
                            RichText::new("Ensures frame-accurate audio-video synchronization across FilmCraft, professional NLEs, and web engines.")
                                .size(9.5)
                                .color(Color32::GRAY),
                        );
                        ui.add_space(10.0);

                        ui.label(RichText::new("BROADCAST LOUDNESS TARGET").size(10.0).strong().color(Color32::from_rgb(140, 160, 185)));
                        ui.add_space(4.0);

                        let targets = [
                            (-23.0, "EBU R128 (-23.0 LUFS) • European Broadcast Standard"),
                            (-24.0, "ATSC A/85 (-24.0 LUFS) • North American Television Standard"),
                            (-14.0, "Streaming Web / Podcast (-14.0 LUFS) • Spotify / YouTube / Apple"),
                        ];
                        for (lufs, desc) in targets {
                            let selected = (current_prefs.target_loudness_lufs - lufs).abs() < 0.1;
                            if ui.radio(selected, desc).clicked() {
                                current_prefs.target_loudness_lufs = lufs;
                                changed = true;
                            }
                        }
                        ui.add_space(10.0);

                        ui.label(RichText::new("INTER-APPLICATION DAEMON HOOKS").size(10.0).strong().color(Color32::from_rgb(140, 160, 185)));
                        ui.add_space(4.0);

                        if ui.checkbox(&mut current_prefs.auto_pause_on_playback, "Auto-pause queue when external NLE playback is active (craft-daemon hook)").changed() {
                            changed = true;
                        }
                        ui.label(
                            RichText::new("Automatically suspends background encodes to free GPU/CPU bandwidth during live editing in FilmCraft/SoundCraft.")
                                .size(9.5)
                                .color(Color32::GRAY),
                        );
                    }
                }

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(6.0);

                // Modal Footer
                let btn_style = craft_theme::craft_icon_button_style();
                let accent_style = craft_theme::craft_accent_button_style();

                ui.horizontal(|ui| {
                    if craft_ui::buttons::IconButton::new("Reset", Vec2::new(120.0, 24.0), &btn_style)
                        .show(ui, |ui, rect, tint| {
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "Restore Defaults",
                                egui::FontId::proportional(10.5),
                                tint,
                            );
                        })
                        .clicked()
                    {
                        restore_defaults = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if craft_ui::buttons::IconButton::new("Done", Vec2::new(80.0, 24.0), &accent_style)
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

        if restore_defaults {
            current_prefs = settings::MediaCraftPreferences::default();
            changed = true;
        }

        if changed {
            self.settings_store.update(|prefs| {
                *prefs = current_prefs;
            });
        }

        if close {
            self.preferences_open = false;
        }
    }

    fn show_quit_modal(&mut self, ctx: &egui::Context) {
        if self.show_quit_confirm {
            egui::Window::new("Transcoding in Progress")
                .fixed_size(Vec2::new(440.0, 160.0))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
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
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                        });
                    });
                });
        }
    }

    fn handle_mac_menu_commands(&mut self, ctx: &egui::Context) {
        let Some(ref menu) = self.mac_menu else {
            return;
        };
        while let Some(cmd) = menu.try_recv() {
            match cmd {
                mac_menu::MacMenuCommand::About => {
                    self.preferences_open = true;
                    self.preferences_tab = 0;
                }
                mac_menu::MacMenuCommand::Preferences => {
                    self.preferences_open = true;
                }
                mac_menu::MacMenuCommand::AddMedia => {
                    self.workspace_mode = WorkspaceMode::Browse;
                    self.library_state.scan_dialog_open = true;
                }
                mac_menu::MacMenuCommand::AddWatchFolder => {
                    self.workspace_mode = WorkspaceMode::Automations;
                }
                mac_menu::MacMenuCommand::ExportSettings => {
                    if self.editing_preset.is_none() {
                        self.editing_preset = self.presets.first().cloned();
                        self.editing_tab = 0;
                    }
                }
                mac_menu::MacMenuCommand::CloseWindow => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                mac_menu::MacMenuCommand::StartQueue => {
                    self.engine_lock()
                        .resume_queue(mediacraft_protocol::PauseReason::UserRequested);
                }
                mac_menu::MacMenuCommand::PauseQueue => {
                    self.engine_lock()
                        .pause_queue(mediacraft_protocol::PauseReason::UserRequested);
                }
                mac_menu::MacMenuCommand::StopQueue => {
                    self.engine_lock()
                        .pause_queue(mediacraft_protocol::PauseReason::UserRequested);
                }
                mac_menu::MacMenuCommand::ResetQueueStatus => {}
                mac_menu::MacMenuCommand::ClearCompletedJobs => {
                    self.engine_lock().clear_completed();
                }
                mac_menu::MacMenuCommand::NewPreset => {
                    self.workspace_mode = WorkspaceMode::Presets;
                    self.preset_state.active_tab = preset_view::PresetWorkspaceTab::Editor;
                }
                mac_menu::MacMenuCommand::ImportPresets
                | mac_menu::MacMenuCommand::ExportPresets => {
                    self.workspace_mode = WorkspaceMode::Presets;
                }
                mac_menu::MacMenuCommand::ViewWorkstation => {
                    self.workspace_mode = WorkspaceMode::Browse;
                }
                mac_menu::MacMenuCommand::ViewBrowser => {
                    self.workspace_mode = WorkspaceMode::Browse;
                }
                mac_menu::MacMenuCommand::ViewInspector => {
                    self.workspace_mode = WorkspaceMode::Inspect;
                }
                mac_menu::MacMenuCommand::ViewProcess => {
                    self.workspace_mode = WorkspaceMode::Process;
                }
                mac_menu::MacMenuCommand::ViewQueue => {
                    self.workspace_mode = WorkspaceMode::Queue;
                }
                mac_menu::MacMenuCommand::ViewAutomations => {
                    self.workspace_mode = WorkspaceMode::Automations;
                }
                mac_menu::MacMenuCommand::ViewDeliveries => {
                    self.workspace_mode = WorkspaceMode::Deliver;
                }
                mac_menu::MacMenuCommand::ViewPresets => {
                    self.workspace_mode = WorkspaceMode::Presets;
                }
                mac_menu::MacMenuCommand::HelpDocs | mac_menu::MacMenuCommand::HelpShortcuts => {
                    self.preferences_open = true;
                    self.preferences_tab = 3;
                }
                mac_menu::MacMenuCommand::Unknown(_) => {}
            }
        }
    }
}

impl eframe::App for MediaCraftApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_mac_menu_commands(ctx);
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

        // Compact application chrome: identity, current collection, global search and actions.
        egui::containers::panel::Panel::top("mediacraft_top_bar")
            .exact_size(48.0)
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(17, 20, 23)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    #[cfg(target_os = "macos")]
                    ui.add_space(72.0);

                    // Product Icon (mediacraft-icon.png embedded asset)
                    let icon_size = Vec2::new(30.0, 30.0);
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

                    ui.add_space(18.0);

                    // Global Search Bar
                    ui.label(RichText::new("🔍").size(12.0).color(Color32::GRAY));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Search files, tags, people...")
                            .desired_width(ui.available_width().min(420.0)),
                    );

                    // Right Toolbar Actions using craft_ui::buttons::IconButton
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn_style = craft_theme::craft_icon_button_style();
                        let btn_size = Vec2::new(120.0, 28.0);
                        if craft_ui::buttons::IconButton::new("Scan / Import", btn_size, &btn_style)
                            .selected(self.library_state.scan_dialog_open)
                            .show(ui, |ui, rect, tint| {
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    "⤓ Scan / Import",
                                    egui::FontId::proportional(11.0),
                                    tint,
                                );
                            })
                            .clicked()
                        {
                            self.workspace_mode = WorkspaceMode::Browse;
                            self.library_state.scan_dialog_open = true;
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
                        let ipc_active = self.ipc_server.is_some();
                        let status_col = if ipc_active {
                            Color32::from_rgb(90, 230, 130)
                        } else {
                            Color32::GRAY
                        };
                        ui.label(
                            RichText::new(if ipc_active {
                                "● IPC Active"
                            } else {
                                "○ IPC Inactive"
                            })
                            .size(11.0)
                            .color(status_col),
                        );
                    });
                });
            });

        egui::containers::panel::Panel::left("mediacraft_navigation")
            .exact_size(212.0)
            .resizable(true)
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(17, 20, 23))
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("WORKSPACES")
                            .size(10.0)
                            .strong()
                            .color(Color32::from_rgb(130, 145, 160)),
                    );
                });
                ui.add_space(5.0);
                for (label, mode) in [
                    ("▦  Browse", WorkspaceMode::Browse),
                    ("▤  Collections", WorkspaceMode::Collections),
                    ("▷  Process", WorkspaceMode::Process),
                    ("☷  Queue", WorkspaceMode::Queue),
                    ("⌘  Automations", WorkspaceMode::Automations),
                ] {
                    let selected = self.workspace_mode == mode;
                    if ui
                        .selectable_label(selected, RichText::new(label).size(12.0))
                        .clicked()
                    {
                        self.workspace_mode = mode;
                    }
                }

                ui.add_space(8.0);
                ui.separator();
                for (label, mode) in [
                    ("⇧  Deliver", WorkspaceMode::Deliver),
                    ("⚙  Presets", WorkspaceMode::Presets),
                ] {
                    if ui
                        .selectable_label(
                            self.workspace_mode == mode,
                            RichText::new(label).size(12.0),
                        )
                        .clicked()
                    {
                        self.workspace_mode = mode;
                    }
                }

                ui.add_space(10.0);
                ui.separator();
                if matches!(
                    self.workspace_mode,
                    WorkspaceMode::Browse | WorkspaceMode::Collections | WorkspaceMode::Inspect
                ) {
                    let counts = self.catalog.kind_counts();
                    let total = self.catalog.all_assets().len();
                    render_sidebar_nav(ui, &mut self.sidebar_state, &counts, total);
                }
            });

        egui::containers::panel::Panel::bottom("mediacraft_activity_drawer")
            .frame(
                egui::Frame::NONE
                    .fill(Color32::from_rgb(17, 20, 23))
                    .inner_margin(egui::Margin::symmetric(10, 4)),
            )
            .show(ui, |ui| {
                render_queue_drawer(ui, &mut self.drawer_state, &self.engine)
            });

        egui::CentralPanel::default().show(ui, |ui| match self.workspace_mode {
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

                let library_action = render_library_view(
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
                        previews: &mut self.previews,
                    },
                );
                self.handle_library_action(library_action);
            }
            WorkspaceMode::Process => {
                let selected_asset = self
                    .selected_asset_id
                    .as_deref()
                    .and_then(|id| self.catalog.get_by_id(id));
                let mut enqueue_job_action: Option<(EncodingPreset, String, String)> = None;

                ui.columns(3, |columns| {
                    for idx in 0..2 {
                        let column = &mut columns[idx];
                        let (label, output) = if idx == 0 {
                            ("SOURCE", false)
                        } else {
                            ("OUTPUT PREVIEW", true)
                        };
                        column.heading(label);
                        column.separator();
                        if let Some(asset) = selected_asset {
                            let preview_width = column.available_width();
                            let (rect, _) = column.allocate_exact_size(
                                Vec2::new(preview_width, preview_width * 9.0 / 16.0),
                                egui::Sense::hover(),
                            );
                            let painter = column.painter_at(rect);
                            media_preview::paint_asset_preview(
                                &mut self.previews,
                                column.ctx(),
                                &painter,
                                rect,
                                asset,
                                0.5,
                            );
                            if output {
                                painter.rect_filled(
                                    rect,
                                    CornerRadius::same(3),
                                    Color32::from_black_alpha(35),
                                );
                                painter.text(
                                    rect.left_top() + Vec2::new(8.0, 8.0),
                                    egui::Align2::LEFT_TOP,
                                    "OUTPUT PREVIEW • NOT GENERATED",
                                    FontId::proportional(10.0),
                                    Color32::from_rgb(34, 218, 215),
                                );
                            }
                            column.label(RichText::new(&asset.filename).strong());
                            if let Some(video) = &asset.video {
                                column.label(format!(
                                    "{} × {}  •  {:.3} fps  •  {}",
                                    video.width, video.height, video.fps, video.codec
                                ));
                            }
                            if output {
                                column.label("Output appears here after an encode completes.");
                            }
                        } else {
                            column.centered_and_justified(|ui| {
                                ui.label("Select a video in Browse to compare source and output.")
                            });
                        }
                    }
                    render_process_panel(
                        &mut columns[2],
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
            }
            WorkspaceMode::Browse | WorkspaceMode::Inspect => {
                // 2. Central Docking Workspace Area
                let mut style = DockStyle::from_ui(ui);
                style.tab_height = 26.0;
                style.background = Color32::from_rgb(16, 19, 24);
                style.tab_background = Color32::from_rgb(25, 30, 38);
                style.active_background = Color32::from_rgb(42, 50, 64);
                style.text = Color32::from_rgb(235, 245, 255);
                style.inactive_text = Color32::from_rgb(160, 175, 195);
                style.accent = Color32::from_rgb(27, 211, 207);
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
                let mut library_action = None;

                let output = DockArea::new(egui::Id::new("mediacraft_workspace_dock")).show(
                    ui,
                    &layout,
                    &style,
                    |panel| panel.label().into(),
                    |_| Permissions::default(),
                    |ui, panel| match *panel {
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
                                    previews: &mut self.previews,
                                },
                            );
                        }
                        Panel::Inspector => {
                            render_inspector_panel(
                                ui,
                                &mut self.inspector_state,
                                selected_asset,
                                self.catalog.graph(),
                                &mut self.previews,
                            );
                        }
                    },
                );

                for action in output.actions {
                    let _ = layout.apply(action);
                }
                self.layout = layout;
                self.handle_library_action(library_action);
            }
        });

        // 4. Modal Export Settings Window (if triggered)
        self.show_export_settings_modal(ui.ctx());

        // 5. Preferences Modal (via craft-settings)
        self.show_preferences_modal(ui.ctx());

        // 6. Safe Quit Confirmation Dialog (via craft-quit)
        self.show_quit_modal(ui.ctx());
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
    #[allow(unused_mut)]
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(Vec2::new(1440.0, 900.0))
        .with_min_inner_size(Vec2::new(960.0, 600.0))
        .with_title("MediaCraft")
        .with_icon(app_icon::window_icon());

    #[cfg(target_os = "macos")]
    {
        viewport = viewport
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false);
    }
    #[cfg(not(target_os = "macos"))]
    {
        viewport = viewport.with_decorations(false);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "MediaCraft",
        native_options,
        Box::new(|cc| Ok(Box::new(MediaCraftApp::new(cc)))),
    )
}
