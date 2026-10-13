//! Application icon loader for MediaCraft window, taskbar/Dock, and UI chrome.

#[cfg(target_os = "macos")]
pub const DOCK_ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon/mediacraft-1024.png");
#[cfg(not(target_os = "macos"))]
pub const DOCK_ICON_PNG: &[u8] = include_bytes!("../../../docs/mediacraft-icon.png");

pub const CHROME_ICON_PNG: &[u8] = include_bytes!("../../../docs/mediacraft-icon.png");

/// Window, taskbar, and macOS Dock icon.
/// On macOS, uses the standard Apple HIG 1024x1024 canvas with 825x825 squircle and 99px margin
/// matching PhotoCraft and native Apple Dock guidelines.
pub fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(DOCK_ICON_PNG).unwrap_or_default()
}

/// Decodes the tight squircle icon for in-app UI chrome (top bar header).
pub fn app_icon_image() -> egui::ColorImage {
    let icon = eframe::icon_data::from_png_bytes(CHROME_ICON_PNG).unwrap_or_default();
    if icon.width > 0 && icon.height > 0 && !icon.rgba.is_empty() {
        egui::ColorImage::from_rgba_unmultiplied(
            [icon.width as usize, icon.height as usize],
            &icon.rgba,
        )
    } else {
        egui::ColorImage::example()
    }
}
