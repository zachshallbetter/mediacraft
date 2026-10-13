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

/// Accent / Primary action button style.
pub fn craft_accent_button_style() -> IconButtonStyle {
    IconButtonStyle {
        normal: ButtonVisuals {
            fill: Color32::from_rgb(22, 65, 45),
            stroke: Stroke::new(1.0, Color32::from_rgb(45, 120, 80)),
            icon: Color32::from_rgb(140, 240, 180),
        },
        hovered: ButtonVisuals {
            fill: Color32::from_rgb(28, 85, 58),
            stroke: Stroke::new(1.0, Color32::from_rgb(60, 160, 110)),
            icon: Color32::from_rgb(180, 255, 210),
        },
        pressed: Some(ButtonVisuals {
            fill: Color32::from_rgb(34, 105, 72),
            stroke: Stroke::new(1.2, Color32::from_rgb(80, 200, 140)),
            icon: Color32::WHITE,
        }),
        selected: ButtonVisuals {
            fill: Color32::from_rgb(34, 105, 72),
            stroke: Stroke::new(1.2, Color32::from_rgb(80, 200, 140)),
            icon: Color32::WHITE,
        },
        corner_radius: CornerRadius::same(4),
        focus_stroke: Stroke::new(1.5, Color32::from_rgb(90, 230, 160)),
    }
}

/// Danger / Destructive action button style.
pub fn craft_danger_button_style() -> IconButtonStyle {
    IconButtonStyle {
        normal: ButtonVisuals {
            fill: Color32::from_rgb(55, 24, 28),
            stroke: Stroke::new(1.0, Color32::from_rgb(100, 42, 48)),
            icon: Color32::from_rgb(255, 150, 160),
        },
        hovered: ButtonVisuals {
            fill: Color32::from_rgb(75, 30, 36),
            stroke: Stroke::new(1.0, Color32::from_rgb(140, 55, 65)),
            icon: Color32::from_rgb(255, 190, 200),
        },
        pressed: Some(ButtonVisuals {
            fill: Color32::from_rgb(95, 36, 44),
            stroke: Stroke::new(1.2, Color32::from_rgb(180, 70, 80)),
            icon: Color32::WHITE,
        }),
        selected: ButtonVisuals {
            fill: Color32::from_rgb(95, 36, 44),
            stroke: Stroke::new(1.2, Color32::from_rgb(180, 70, 80)),
            icon: Color32::WHITE,
        },
        corner_radius: CornerRadius::same(4),
        focus_stroke: Stroke::new(1.5, Color32::from_rgb(255, 100, 110)),
    }
}

/// Small compact button style for table row actions.
pub fn craft_small_button_style() -> IconButtonStyle {
    IconButtonStyle {
        normal: ButtonVisuals {
            fill: Color32::from_rgb(22, 26, 32),
            stroke: Stroke::new(1.0, Color32::from_rgb(40, 46, 56)),
            icon: Color32::from_rgb(160, 175, 195),
        },
        hovered: ButtonVisuals {
            fill: Color32::from_rgb(32, 40, 52),
            stroke: Stroke::new(1.0, Color32::from_rgb(60, 80, 110)),
            icon: Color32::WHITE,
        },
        pressed: Some(ButtonVisuals {
            fill: Color32::from_rgb(40, 50, 68),
            stroke: Stroke::new(1.0, Color32::from_rgb(80, 110, 150)),
            icon: Color32::WHITE,
        }),
        selected: ButtonVisuals {
            fill: Color32::from_rgb(24, 48, 80),
            stroke: Stroke::new(1.0, Color32::from_rgb(65, 140, 210)),
            icon: Color32::from_rgb(90, 200, 255),
        },
        corner_radius: CornerRadius::same(3),
        focus_stroke: Stroke::new(1.0, Color32::from_rgb(80, 180, 255)),
    }
}
