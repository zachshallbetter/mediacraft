//! Craft Suite Theme & Component Styling Integration

use craft_ui::buttons::{ButtonVisuals, IconButtonStyle};
use craft_ui::theme::WidgetPalette;
use egui::{Color32, CornerRadius, Stroke};

/// Unified Craft Suite widget palette for egui interactive controls.
pub fn craft_widget_palette() -> WidgetPalette {
    WidgetPalette {
        noninteractive: Color32::from_rgb(18, 21, 26),
        inactive: Color32::from_rgb(28, 33, 40),
        hovered: Color32::from_rgb(45, 54, 66),
        active: Color32::from_rgb(60, 72, 88),
        open: Color32::from_rgb(28, 33, 40),
    }
}

/// Craft Suite icon button style matching PhotoCraft, VectorCraft, and SoundCraft.
pub fn craft_icon_button_style() -> IconButtonStyle {
    IconButtonStyle {
        normal: ButtonVisuals {
            fill: Color32::from_rgb(26, 31, 40),
            stroke: Stroke::new(1.0, Color32::from_rgb(46, 54, 68)),
            icon: Color32::from_rgb(190, 205, 225),
        },
        hovered: ButtonVisuals {
            fill: Color32::from_rgb(38, 46, 60),
            stroke: Stroke::new(1.0, Color32::from_rgb(70, 95, 130)),
            icon: Color32::from_rgb(240, 248, 255),
        },
        pressed: Some(ButtonVisuals {
            fill: Color32::from_rgb(45, 58, 78),
            stroke: Stroke::new(1.0, Color32::from_rgb(90, 130, 180)),
            icon: Color32::WHITE,
        }),
        selected: ButtonVisuals {
            fill: Color32::from_rgb(26, 58, 98),
            stroke: Stroke::new(1.2, Color32::from_rgb(75, 170, 245)),
            icon: Color32::from_rgb(90, 200, 255),
        },
        corner_radius: CornerRadius::same(4),
        focus_stroke: Stroke::new(1.5, Color32::from_rgb(80, 180, 255)),
    }
}
