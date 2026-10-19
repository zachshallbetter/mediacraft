//! Native macOS Menu Bar via muda, providing professional media workstation top-level menu hierarchy.
//!
//! Provides top-level system menus (MediaCraft, File, Edit, Queue, Preset, View, Window, Help)
//! in the macOS system menu bar rather than being trapped in an in-window frame.
//!
//! Quit (⌘Q) routes through `craft_quit::QUIT_MENU_ID` so any unsaved or actively running
//! encodes prompt the user safely before termination.

use std::sync::mpsc::{Receiver, channel};

#[cfg(target_os = "macos")]
use std::cell::RefCell;

#[cfg(target_os = "macos")]
use muda::accelerator::{Accelerator, Code, Modifiers};
#[cfg(target_os = "macos")]
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

#[cfg(target_os = "macos")]
thread_local! {
    /// Keep the installed native menu alive for the lifetime of the process.
    static MENU_BAR: RefCell<Option<Menu>> = const { RefCell::new(None) };
}

#[cfg(target_os = "macos")]
fn parse_accel(s: &str) -> Option<Accelerator> {
    let mapped = s.replace("Cmd+", "CmdOrCtrl+");
    mapped.parse().ok()
}

/// Commands dispatched from native macOS menus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacMenuCommand {
    About,
    Preferences,
    AddMedia,
    AddWatchFolder,
    ExportSettings,
    CloseWindow,
    StartQueue,
    PauseQueue,
    StopQueue,
    ResetQueueStatus,
    ClearCompletedJobs,
    NewPreset,
    ImportPresets,
    ExportPresets,
    ViewWorkstation,
    ViewBrowser,
    ViewInspector,
    ViewProcess,
    ViewQueue,
    ViewAutomations,
    ViewDeliveries,
    ViewPresets,
    HelpDocs,
    HelpShortcuts,
    Unknown(String),
}

impl MacMenuCommand {
    pub fn from_id(id: &str) -> Self {
        match id {
            "app.about" => Self::About,
            "app.preferences" | "edit.preferences" => Self::Preferences,
            "file.add_media" => Self::AddMedia,
            "file.add_watch_folder" => Self::AddWatchFolder,
            "file.export_settings" => Self::ExportSettings,
            "file.close" => Self::CloseWindow,
            "queue.start" => Self::StartQueue,
            "queue.pause" => Self::PauseQueue,
            "queue.stop" => Self::StopQueue,
            "queue.reset" => Self::ResetQueueStatus,
            "queue.clear_completed" => Self::ClearCompletedJobs,
            "preset.new" => Self::NewPreset,
            "preset.import" => Self::ImportPresets,
            "preset.export" => Self::ExportPresets,
            "view.workstation" => Self::ViewWorkstation,
            "view.browser" => Self::ViewBrowser,
            "view.inspector" => Self::ViewInspector,
            "view.process" => Self::ViewProcess,
            "view.queue" => Self::ViewQueue,
            "view.automations" => Self::ViewAutomations,
            "view.deliveries" => Self::ViewDeliveries,
            "view.presets" => Self::ViewPresets,
            "help.docs" => Self::HelpDocs,
            "help.shortcuts" => Self::HelpShortcuts,
            other => Self::Unknown(other.to_string()),
        }
    }
}

pub struct MacMenu {
    rx: Receiver<MacMenuCommand>,
}

impl MacMenu {
    #[cfg(target_os = "macos")]
    pub fn install(ctx: &egui::Context) -> Option<Self> {
        if MENU_BAR.with_borrow(Option::is_some) {
            return None;
        }

        let bar = Menu::new();

        // 1. Application Menu: MediaCraft
        let app_menu = Submenu::new("MediaCraft", true);
        let about_item = MenuItem::with_id("app.about", "About MediaCraft", true, None);
        let prefs_item = MenuItem::with_id(
            "app.preferences",
            "Settings...",
            true,
            Some(Accelerator::new(Modifiers::META, Code::Comma)),
        );
        let quit_item = MenuItem::with_id(
            craft_quit::QUIT_MENU_ID,
            "Quit MediaCraft",
            true,
            Some(Accelerator::new(Modifiers::META, Code::KeyQ)),
        );

        let _ = app_menu.append_items(&[
            &about_item,
            &PredefinedMenuItem::separator(),
            &prefs_item,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(Some("Hide MediaCraft")),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &quit_item,
        ]);
        let _ = bar.append(&app_menu);

        // 2. File Menu
        let file_menu = Submenu::new("File", true);
        let add_media_item = MenuItem::with_id(
            "file.add_media",
            "Add Source...",
            true,
            parse_accel("Cmd+I"),
        );
        let add_watch_folder_item = MenuItem::with_id(
            "file.add_watch_folder",
            "Add Watch Folder...",
            true,
            parse_accel("Cmd+Shift+W"),
        );
        let export_settings_item = MenuItem::with_id(
            "file.export_settings",
            "Export Settings...",
            true,
            parse_accel("Cmd+M"),
        );
        let close_item =
            MenuItem::with_id("file.close", "Close Window", true, parse_accel("Cmd+W"));

        let _ = file_menu.append_items(&[
            &add_media_item,
            &add_watch_folder_item,
            &PredefinedMenuItem::separator(),
            &export_settings_item,
            &PredefinedMenuItem::separator(),
            &close_item,
        ]);
        let _ = bar.append(&file_menu);

        // 3. Edit Menu
        let edit_menu = Submenu::new("Edit", true);
        let edit_prefs_item = MenuItem::with_id(
            "edit.preferences",
            "Preferences...",
            true,
            parse_accel("Cmd+Comma"),
        );
        let _ = edit_menu.append_items(&[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::select_all(None),
            &PredefinedMenuItem::separator(),
            &edit_prefs_item,
        ]);
        let _ = bar.append(&edit_menu);

        // 4. Queue Menu
        let queue_menu = Submenu::new("Queue", true);
        let start_queue_item = MenuItem::with_id(
            "queue.start",
            "Start Queue",
            true,
            parse_accel("Cmd+Return"),
        );
        let pause_queue_item = MenuItem::with_id("queue.pause", "Pause Queue", true, None);
        let stop_queue_item =
            MenuItem::with_id("queue.stop", "Stop Queue", true, parse_accel("Cmd+Period"));
        let reset_status_item = MenuItem::with_id("queue.reset", "Reset Status", true, None);
        let clear_completed_item =
            MenuItem::with_id("queue.clear_completed", "Clear Completed Jobs", true, None);

        let _ = queue_menu.append_items(&[
            &start_queue_item,
            &pause_queue_item,
            &stop_queue_item,
            &PredefinedMenuItem::separator(),
            &reset_status_item,
            &clear_completed_item,
        ]);
        let _ = bar.append(&queue_menu);

        // 5. Preset Menu
        let preset_menu = Submenu::new("Preset", true);
        let new_preset_item =
            MenuItem::with_id("preset.new", "Create Preset...", true, parse_accel("Cmd+N"));
        let import_presets_item = MenuItem::with_id(
            "preset.import",
            "Import Presets...",
            true,
            parse_accel("Cmd+O"),
        );
        let export_presets_item =
            MenuItem::with_id("preset.export", "Export Presets...", true, None);

        let _ = preset_menu.append_items(&[
            &new_preset_item,
            &PredefinedMenuItem::separator(),
            &import_presets_item,
            &export_presets_item,
        ]);
        let _ = bar.append(&preset_menu);

        // 6. View Menu
        let view_menu = Submenu::new("View", true);
        let view_workstation = MenuItem::with_id(
            "view.workstation",
            "Workstation",
            true,
            parse_accel("Cmd+1"),
        );
        let view_browser =
            MenuItem::with_id("view.browser", "Media Browser", true, parse_accel("Cmd+2"));
        let view_inspector = MenuItem::with_id(
            "view.inspector",
            "Preview & Inspector",
            true,
            parse_accel("Cmd+3"),
        );
        let view_process = MenuItem::with_id(
            "view.process",
            "Process & Encoding",
            true,
            parse_accel("Cmd+4"),
        );
        let view_queue =
            MenuItem::with_id("view.queue", "Encoding Queue", true, parse_accel("Cmd+5"));
        let view_automations = MenuItem::with_id(
            "view.automations",
            "Watch Folders & Automations",
            true,
            parse_accel("Cmd+6"),
        );
        let view_deliveries =
            MenuItem::with_id("view.deliveries", "Deliveries", true, parse_accel("Cmd+7"));
        let view_presets = MenuItem::with_id(
            "view.presets",
            "Presets Catalog",
            true,
            parse_accel("Cmd+8"),
        );

        let _ = view_menu.append_items(&[
            &view_workstation,
            &view_browser,
            &view_inspector,
            &view_process,
            &view_queue,
            &view_automations,
            &view_deliveries,
            &view_presets,
        ]);
        let _ = bar.append(&view_menu);

        // 7. Window Menu
        let window_menu = Submenu::new("Window", true);
        let _ = window_menu.append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::zoom(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::bring_all_to_front(None),
        ]);
        window_menu.set_as_windows_menu_for_nsapp();
        let _ = bar.append(&window_menu);

        // 8. Help Menu
        let help_menu = Submenu::new("Help", true);
        let help_docs_item = MenuItem::with_id("help.docs", "MediaCraft Documentation", true, None);
        let help_shortcuts_item =
            MenuItem::with_id("help.shortcuts", "Keyboard Shortcuts", true, None);
        let _ = help_menu.append_items(&[
            &help_docs_item,
            &help_shortcuts_item,
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id("app.about", "About MediaCraft", true, None),
        ]);
        help_menu.set_as_help_menu_for_nsapp();
        let _ = bar.append(&help_menu);

        // Register with NSApplication
        bar.init_for_nsapp();

        let (tx, rx) = channel();
        let ctx_clone = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let id = event.id().as_ref();
            if !craft_quit::on_menu_event(&ctx_clone, id) {
                let cmd = MacMenuCommand::from_id(id);
                let _ = tx.send(cmd);
            }
            ctx_clone.request_repaint();
        }));

        MENU_BAR.set(Some(bar));
        Some(Self { rx })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn install(_ctx: &egui::Context) -> Option<Self> {
        None
    }

    pub fn try_recv(&self) -> Option<MacMenuCommand> {
        self.rx.try_recv().ok()
    }
}
