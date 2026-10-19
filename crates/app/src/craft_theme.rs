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

/// Comprehensive color tokens for the MediaCraft workstation.
#[derive(Clone, Copy, Debug)]
pub struct ThemeTokens {
    pub workspace_bg: Color32,
    pub panel_bg: Color32,
    pub panel_header_bg: Color32,
    pub recessed_bg: Color32,
    pub border_subtle: Color32,
    pub border_active: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_disabled: Color32,
    pub accent_blue: Color32,
    pub accent_cyan: Color32,
    pub accent_green: Color32,
    pub accent_danger: Color32,
    pub accent_amber: Color32,
}

impl Default for ThemeTokens {
    fn default() -> Self {
        Self {
            workspace_bg: Color32::from_rgb(14, 16, 21),    // #0E1015
            panel_bg: Color32::from_rgb(21, 24, 33),        // #151821
            panel_header_bg: Color32::from_rgb(26, 29, 39), // #1A1D27
            recessed_bg: Color32::from_rgb(10, 12, 16),     // #0A0C10
            border_subtle: Color32::from_rgb(34, 40, 54),   // #222836
            border_active: Color32::from_rgb(52, 64, 88),   // #344058
            text_primary: Color32::from_rgb(237, 242, 247), // #EDF2F7
            text_secondary: Color32::from_rgb(140, 150, 168), // #8C96A8
            text_disabled: Color32::from_rgb(74, 85, 104),  // #4A5568
            accent_blue: Color32::from_rgb(29, 104, 189),   // #1D68BD
            accent_cyan: Color32::from_rgb(0, 180, 216),    // #00B4D8
            accent_green: Color32::from_rgb(22, 163, 74),   // #16A34A
            accent_danger: Color32::from_rgb(220, 38, 38),  // #DC2626
            accent_amber: Color32::from_rgb(245, 158, 11),  // #F59E0B
        }
    }
}

/// Applies the full MediaCraft workstation dark theme to egui.
pub fn apply_global_theme(ctx: &egui::Context) {
    let tokens = ThemeTokens::default();

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = tokens.workspace_bg;
    visuals.window_fill = tokens.panel_bg;
    visuals.extreme_bg_color = tokens.recessed_bg;
    visuals.faint_bg_color = tokens.panel_header_bg;
    visuals.window_stroke = Stroke::new(1.0, tokens.border_subtle);
    visuals.window_corner_radius = CornerRadius::same(6);
    visuals.menu_corner_radius = CornerRadius::same(6);
    visuals.selection.bg_fill = tokens.accent_blue.gamma_multiply(0.6);
    visuals.selection.stroke = Stroke::new(1.0, tokens.accent_blue);
    visuals.hyperlink_color = tokens.accent_cyan;
    visuals.override_text_color = Some(tokens.text_primary);

    let palette = WidgetPalette {
        noninteractive: tokens.panel_bg,
        inactive: Color32::from_rgb(26, 30, 42),
        hovered: Color32::from_rgb(38, 45, 62),
        active: Color32::from_rgb(48, 58, 80),
        open: tokens.panel_header_bg,
    };
    palette.apply_to(
        &mut visuals.widgets,
        CornerRadius::same(4),
        Stroke::new(1.0, tokens.text_primary),
    );
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, tokens.border_subtle);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(40, 46, 62));
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, tokens.border_active);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, tokens.accent_blue);

    ctx.set_visuals(visuals);

    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(7.0, 3.0);
        s.spacing.interact_size = egui::vec2(24.0, 24.0);
        s.spacing.menu_margin = egui::Margin::same(6);
    });
}

/// Standard rounded card frame for workstation quadrants.
pub fn panel_card_frame() -> egui::Frame {
    let tokens = ThemeTokens::default();
    egui::Frame {
        inner_margin: egui::Margin::same(0),
        outer_margin: egui::Margin::same(0),
        corner_radius: CornerRadius::same(6),
        shadow: egui::epaint::Shadow::NONE,
        fill: tokens.panel_bg,
        stroke: Stroke::new(1.0, tokens.border_subtle),
    }
}

/// Header tab strip frame inside a workstation panel.
pub fn panel_header_frame() -> egui::Frame {
    let tokens = ThemeTokens::default();
    egui::Frame {
        inner_margin: egui::Margin::symmetric(10, 6),
        outer_margin: egui::Margin::same(0),
        corner_radius: CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        },
        shadow: egui::epaint::Shadow::NONE,
        fill: tokens.panel_header_bg,
        stroke: Stroke::NONE,
    }
}

/// Recessed well frame for video previews, scrubber, and data tables.
pub fn recessed_well_frame() -> egui::Frame {
    let tokens = ThemeTokens::default();
    egui::Frame {
        inner_margin: egui::Margin::same(4),
        outer_margin: egui::Margin::same(0),
        corner_radius: CornerRadius::same(4),
        shadow: egui::epaint::Shadow::NONE,
        fill: tokens.recessed_bg,
        stroke: Stroke::new(1.0, tokens.border_subtle),
    }
}
