//! Bottom Activity Drawer (Queue, Watch Folders, IPC Interchange)

use egui::{Color32, RichText};
use mediacraft_daemon::QueueEngine;
use mediacraft_protocol::JobState;
use std::sync::{Arc, Mutex, PoisonError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawerTab {
    Queue,
    WatchFolders,
    Interchange,
}

pub struct QueueDrawerState {
    pub is_expanded: bool,
    pub active_tab: DrawerTab,
}

impl Default for QueueDrawerState {
    fn default() -> Self {
        Self {
            is_expanded: true,
            active_tab: DrawerTab::Queue,
        }
    }
}

pub fn render_queue_drawer(
    ui: &mut egui::Ui,
    state: &mut QueueDrawerState,
    engine: &Arc<Mutex<QueueEngine>>,
) {
    let eng = engine.lock().unwrap_or_else(PoisonError::into_inner);
    let status = eng.status();

    // 1. Drawer Header Bar (always visible)
    ui.horizontal(|ui| {
        let expand_icon = if state.is_expanded { "▼" } else { "▲" };
        if ui
            .button(
                RichText::new(format!("{expand_icon} Activity Drawer"))
                    .size(11.5)
                    .strong(),
            )
            .clicked()
        {
            state.is_expanded = !state.is_expanded;
        }

        ui.add_space(8.0);

        if ui
            .selectable_label(
                state.active_tab == DrawerTab::Queue,
                format!("Queue ({})", status.active_jobs + status.queued_jobs),
            )
            .clicked()
        {
            state.active_tab = DrawerTab::Queue;
            state.is_expanded = true;
        }
        if ui
            .selectable_label(
                state.active_tab == DrawerTab::WatchFolders,
                "Watch Folders (2)",
            )
            .clicked()
        {
            state.active_tab = DrawerTab::WatchFolders;
            state.is_expanded = true;
        }
        if ui
            .selectable_label(state.active_tab == DrawerTab::Interchange, "Suite IPC")
            .clicked()
        {
            state.active_tab = DrawerTab::Interchange;
            state.is_expanded = true;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (status_text, color) = if status.pause_reason.is_some() {
                ("PAUSED", Color32::from_rgb(255, 180, 50))
            } else if status.active_jobs > 0 {
                ("ENCODING", Color32::from_rgb(80, 230, 120))
            } else {
                ("IDLE", Color32::GRAY)
            };

            ui.label(RichText::new(status_text).size(11.0).color(color).strong());
            ui.label(
                RichText::new("Queue Status:")
                    .size(10.5)
                    .color(Color32::GRAY),
            );
        });
    });

    if !state.is_expanded {
        return;
    }

    ui.separator();

    // 2. Drawer Body
    match state.active_tab {
        DrawerTab::Queue => {
            let jobs = eng.list_jobs();
            if jobs.is_empty() {
                ui.label(
                    RichText::new("Queue is currently empty.")
                        .size(11.0)
                        .italics()
                        .color(Color32::GRAY),
                );
                return;
            }

            egui::ScrollArea::vertical()
                .id_salt("drawer_queue_scroll")
                .max_height(140.0)
                .show(ui, |ui| {
                    for job in jobs {
                        ui.horizontal(|ui| {
                            let (state_str, state_color) = match job.state {
                                JobState::Queued => ("Queued", Color32::LIGHT_GRAY),
                                JobState::Running { .. } => {
                                    ("Encoding", Color32::from_rgb(80, 230, 120))
                                }
                                JobState::Paused { .. } => {
                                    ("Paused", Color32::from_rgb(255, 180, 50))
                                }
                                JobState::Completed { .. } => {
                                    ("Done", Color32::from_rgb(70, 160, 240))
                                }
                                JobState::Failed { .. } => {
                                    ("Failed", Color32::from_rgb(255, 70, 70))
                                }
                                JobState::Cancelled => ("Cancelled", Color32::GRAY),
                            };

                            ui.label(
                                RichText::new(state_str)
                                    .size(11.0)
                                    .strong()
                                    .color(state_color),
                            );
                            ui.label(
                                RichText::new(
                                    job.source_path
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("source"),
                                )
                                .size(11.5)
                                .strong(),
                            );
                            ui.label(
                                RichText::new(format!("➔ {}", job.preset.name))
                                    .size(10.5)
                                    .color(Color32::from_rgb(140, 180, 220)),
                            );

                            if let JobState::Running { progress_pct, .. } = job.state {
                                ui.add(
                                    egui::ProgressBar::new(progress_pct)
                                        .desired_width(120.0)
                                        .fill(Color32::from_rgb(80, 220, 120)),
                                );
                            }

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(job.output_path.display().to_string())
                                            .size(10.5)
                                            .color(Color32::GRAY),
                                    );
                                },
                            );
                        });
                        ui.add_space(2.0);
                    }
                });
        }
        DrawerTab::WatchFolders => {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Auto-Ingest Watch Folders:")
                        .strong()
                        .size(11.0),
                );
                ui.label(
                    RichText::new("2 active watch paths")
                        .size(10.5)
                        .color(Color32::GRAY),
                );
            });
            ui.label(
                RichText::new(
                    "• /Volumes/Media/Camera_Drop  ➔  Preset: Editing Proxy (H.264 1080p)",
                )
                .size(10.5)
                .color(Color32::from_rgb(180, 205, 235)),
            );
            ui.label(
                RichText::new("• /Volumes/Audio/Stems_Ingest  ➔  Preset: SoundCraft M4A/AAC 320k")
                    .size(10.5)
                    .color(Color32::from_rgb(180, 205, 235)),
            );
        }
        DrawerTab::Interchange => {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("● Socket IPC: /tmp/mediacraft.sock")
                        .size(11.0)
                        .color(Color32::from_rgb(90, 230, 130))
                        .strong(),
                );
                ui.label(
                    RichText::new(
                        "Active Dynamic Link endpoint for FilmCraft, EffectCraft, and SoundCraft",
                    )
                    .size(10.5)
                    .color(Color32::GRAY),
                );
            });
        }
    }
}
