//! Photorealistic embedded textures and procedural fallback renderers for MediaCraft workstation.

use egui::{
    Color32, ColorImage, CornerRadius, Pos2, Rect, Stroke, TextureHandle, TextureOptions, Vec2,
};

static PREVIEW_MOUNTAIN_BYTES: &[u8] =
    include_bytes!("../../../assets/mockup/preview_mountain.png");
static PREVIEW_INTERVIEW_BYTES: &[u8] =
    include_bytes!("../../../assets/mockup/preview_interview.png");
static CLIP_MOUNTAIN_BYTES: &[u8] = include_bytes!("../../../assets/mockup/clip_mountain.png");
static CLIP_CONCERT_BYTES: &[u8] = include_bytes!("../../../assets/mockup/clip_concert.png");
static CLIP_DRONE_BYTES: &[u8] = include_bytes!("../../../assets/mockup/clip_drone.png");
static CLIP_INTERVIEW_BYTES: &[u8] = include_bytes!("../../../assets/mockup/clip_interview.png");
static CLIP_CITY_BYTES: &[u8] = include_bytes!("../../../assets/mockup/clip_city.png");
static CLIP_UNDERWATER_BYTES: &[u8] = include_bytes!("../../../assets/mockup/clip_underwater.png");

static LIB_1_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_1_mountain.png");
static LIB_2_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_2_cabin.png");
static LIB_3_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_3_hiker.png");
static LIB_4_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_4_flower.png");
static LIB_5_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_5_surf.png");
static LIB_6_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_6_mic.png");
static LIB_7_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_7_wireframe.png");
static LIB_8_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_8_drone.png");
static LIB_9_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_9_colorbars.png");
static LIB_10_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_10_skier.png");
static LIB_11_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_11_waterfall.png");
static LIB_12_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_12_portrait.png");
static LIB_13_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_13_food.png");
static LIB_14_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_14_canyon.png");
static LIB_15_BYTES: &[u8] = include_bytes!("../../../assets/mockup/lib_15_car.png");

#[derive(Default)]
pub struct WorkstationTextures {
    pub preview_mountain: Option<TextureHandle>,
    pub preview_interview: Option<TextureHandle>,
    pub clip_mountain: Option<TextureHandle>,
    pub clip_concert: Option<TextureHandle>,
    pub clip_drone: Option<TextureHandle>,
    pub clip_interview: Option<TextureHandle>,
    pub clip_city: Option<TextureHandle>,
    pub clip_underwater: Option<TextureHandle>,
    pub lib_textures: Vec<TextureHandle>,
}

fn decode_png(ctx: &egui::Context, id: &str, bytes: &[u8]) -> Option<TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let raw = img.into_raw();
    let color_image = ColorImage::from_rgba_unmultiplied(size, &raw);
    Some(ctx.load_texture(id, color_image, TextureOptions::LINEAR))
}

impl WorkstationTextures {
    pub fn ensure_loaded(&mut self, ctx: &egui::Context) {
        if self.preview_mountain.is_none() {
            self.preview_mountain = decode_png(ctx, "preview_mountain", PREVIEW_MOUNTAIN_BYTES);
            self.preview_interview = decode_png(ctx, "preview_interview", PREVIEW_INTERVIEW_BYTES);
            self.clip_mountain = decode_png(ctx, "clip_mountain", CLIP_MOUNTAIN_BYTES);
            self.clip_concert = decode_png(ctx, "clip_concert", CLIP_CONCERT_BYTES);
            self.clip_drone = decode_png(ctx, "clip_drone", CLIP_DRONE_BYTES);
            self.clip_interview = decode_png(ctx, "clip_interview", CLIP_INTERVIEW_BYTES);
            self.clip_city = decode_png(ctx, "clip_city", CLIP_CITY_BYTES);
            self.clip_underwater = decode_png(ctx, "clip_underwater", CLIP_UNDERWATER_BYTES);

            let lib_bytes = [
                ("lib_1", LIB_1_BYTES),
                ("lib_2", LIB_2_BYTES),
                ("lib_3", LIB_3_BYTES),
                ("lib_4", LIB_4_BYTES),
                ("lib_5", LIB_5_BYTES),
                ("lib_6", LIB_6_BYTES),
                ("lib_7", LIB_7_BYTES),
                ("lib_8", LIB_8_BYTES),
                ("lib_9", LIB_9_BYTES),
                ("lib_10", LIB_10_BYTES),
                ("lib_11", LIB_11_BYTES),
                ("lib_12", LIB_12_BYTES),
                ("lib_13", LIB_13_BYTES),
                ("lib_14", LIB_14_BYTES),
                ("lib_15", LIB_15_BYTES),
            ];

            self.lib_textures.clear();
            for (id, bytes) in lib_bytes {
                if let Some(tex) = decode_png(ctx, id, bytes) {
                    self.lib_textures.push(tex);
                }
            }
        }
    }

    pub fn get_preview_texture(&self, clip_name: &str) -> Option<&TextureHandle> {
        if clip_name.contains("Interview") {
            self.preview_interview
                .as_ref()
                .or(self.clip_interview.as_ref())
        } else if clip_name.contains("A001") || clip_name.contains("Mountain") {
            self.preview_mountain
                .as_ref()
                .or(self.clip_mountain.as_ref())
        } else {
            self.get_clip_texture(clip_name)
        }
    }

    pub fn get_clip_texture(&self, clip_name: &str) -> Option<&TextureHandle> {
        if clip_name.contains("A001") || clip_name.contains("Mountain") {
            self.clip_mountain.as_ref()
        } else if clip_name.contains("Concert") {
            self.clip_concert.as_ref()
        } else if clip_name.contains("Drone") {
            self.clip_drone.as_ref()
        } else if clip_name.contains("Interview") {
            self.clip_interview.as_ref()
        } else if clip_name.contains("City") {
            self.clip_city.as_ref()
        } else if clip_name.contains("Underwater") {
            self.clip_underwater.as_ref()
        } else {
            self.clip_mountain.as_ref()
        }
    }

    pub fn get_asset_texture(&self, id: &str, filename: &str) -> Option<&TextureHandle> {
        let key = format!("{id}_{filename}");
        let idx = if key.contains("A001_C001") || key.contains("Mountain") {
            Some(0)
        } else if key.contains("B002_C014") || key.contains("Cabin") {
            Some(1)
        } else if key.contains("C003_C007") || key.contains("Hiker") {
            Some(2)
        } else if key.contains("IMG_8342") || key.contains("Flower") {
            Some(3)
        } else if key.contains("Surf_001") || key.contains("Surf") {
            Some(4)
        } else if key.contains("Podcast_Interview") || key.contains("Podcast") {
            Some(5)
        } else if key.contains("Motion_Graphics") || key.contains("Motion") {
            Some(6)
        } else if key.contains("Drone_003") || key.contains("Drone") {
            Some(7)
        } else if key.contains("Color_Bars") {
            Some(8)
        } else if key.contains("Ski_Day_01") || key.contains("Ski") {
            Some(9)
        } else if key.contains("Waterfall") {
            Some(10)
        } else if key.contains("Interview_B_Roll") {
            Some(11)
        } else if key.contains("Product_Shot") {
            Some(12)
        } else if key.contains("Aerial_Utah") {
            Some(13)
        } else if key.contains("Car_Commercial") || key.contains("Car") {
            Some(14)
        } else {
            None
        };

        if let Some(i) = idx {
            self.lib_textures.get(i)
        } else {
            None
        }
    }
}

/// Computes UV coordinates that center-crop (cover) a texture within a target rectangle
/// so that the visual aspect ratio is strictly preserved (1:1 square pixel aspect ratio)
/// without any stretching or squishing.
pub fn calculate_cover_uv(tex_size: [usize; 2], target_rect: Rect) -> Rect {
    let tex_w = tex_size[0] as f32;
    let tex_h = tex_size[1] as f32;
    let target_w = target_rect.width();
    let target_h = target_rect.height();

    if tex_w <= 0.0 || tex_h <= 0.0 || target_w <= 0.0 || target_h <= 0.0 {
        return Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
    }

    let tex_aspect = tex_w / tex_h;
    let target_aspect = target_w / target_h;

    if (tex_aspect - target_aspect).abs() < 0.002 {
        Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0))
    } else if tex_aspect > target_aspect {
        // Texture is wider than target: keep full vertical height, crop horizontal edges symmetrically
        let visible_fraction = target_aspect / tex_aspect;
        let u_margin = ((1.0 - visible_fraction) * 0.5).clamp(0.0, 0.49);
        Rect::from_min_max(Pos2::new(u_margin, 0.0), Pos2::new(1.0 - u_margin, 1.0))
    } else {
        // Texture is taller than target: keep full horizontal width, crop vertical edges symmetrically
        let visible_fraction = tex_aspect / target_aspect;
        let v_margin = ((1.0 - visible_fraction) * 0.5).clamp(0.0, 0.49);
        Rect::from_min_max(Pos2::new(0.0, v_margin), Pos2::new(1.0, 1.0 - v_margin))
    }
}

/// Computes a destination rectangle fitted inside `bounds` that maintains the exact `target_aspect` ratio.
pub fn fit_aspect_rect(bounds: Rect, target_aspect: f32) -> Rect {
    let max_w = bounds.width();
    let max_h = bounds.height();
    if max_w <= 0.0 || max_h <= 0.0 || target_aspect <= 0.0 {
        return bounds;
    }

    let (fit_w, fit_h) = if max_w / max_h > target_aspect {
        (max_h * target_aspect, max_h)
    } else {
        (max_w, max_w / target_aspect)
    };

    let offset_x = (max_w - fit_w) * 0.5;
    let offset_y = (max_h - fit_h) * 0.5;
    Rect::from_min_size(
        Pos2::new(bounds.min.x + offset_x, bounds.min.y + offset_y),
        Vec2::new(fit_w, fit_h),
    )
}

/// Paints a photorealistic asset thumbnail from library textures. Returns true if rendered with a texture.
pub fn paint_asset_thumbnail_with_textures(
    painter: &egui::Painter,
    textures: &WorkstationTextures,
    rect: Rect,
    id: &str,
    filename: &str,
) -> bool {
    if rect.width() < 10.0 || rect.height() < 10.0 {
        return false;
    }
    if let Some(tex) = textures.get_asset_texture(id, filename) {
        let uv = calculate_cover_uv(tex.size(), rect);
        painter.image(tex.id(), rect, uv, Color32::WHITE);
        painter.rect_stroke(
            rect,
            CornerRadius::same(3),
            Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 25)),
            egui::StrokeKind::Inside,
        );
        true
    } else {
        false
    }
}

/// Paints a photorealistic thumbnail using embedded mockup textures with procedural fallback.
pub fn paint_clip_thumbnail_with_textures(
    painter: &egui::Painter,
    textures: &WorkstationTextures,
    rect: Rect,
    clip_name: &str,
) {
    if rect.width() < 10.0 || rect.height() < 10.0 {
        return;
    }

    if let Some(tex) = textures.get_clip_texture(clip_name) {
        let uv = calculate_cover_uv(tex.size(), rect);
        painter.image(tex.id(), rect, uv, Color32::WHITE);
        painter.rect_stroke(
            rect,
            CornerRadius::same(3),
            Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 25)),
            egui::StrokeKind::Inside,
        );
    } else {
        paint_clip_thumbnail(painter, rect, clip_name);
    }
}

/// Paints a rich procedural thumbnail matching the target clip identifier (fallback).
pub fn paint_clip_thumbnail(painter: &egui::Painter, rect: Rect, clip_name: &str) {
    if rect.width() < 10.0 || rect.height() < 10.0 {
        return;
    }

    // Clip background base
    painter.rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(18, 20, 26));

    if clip_name.contains("A001") || clip_name.contains("Mountain") {
        paint_mountain_sunset(painter, rect);
    } else if clip_name.contains("Concert") {
        paint_concert_stage(painter, rect);
    } else if clip_name.contains("Drone") {
        paint_ocean_wave(painter, rect);
    } else if clip_name.contains("Interview") {
        paint_interview_portrait(painter, rect);
    } else if clip_name.contains("City") {
        paint_city_skyline(painter, rect);
    } else if clip_name.contains("Underwater") {
        paint_underwater(painter, rect);
    } else {
        paint_generic_media(painter, rect, clip_name);
    }

    // Subtle inner border stroke
    painter.rect_stroke(
        rect,
        CornerRadius::same(3),
        Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 20)),
        egui::StrokeKind::Inside,
    );
}

/// A001_C012: Dramatic sunset mountain landscape
fn paint_mountain_sunset(painter: &egui::Painter, rect: Rect) {
    let w = rect.width();
    let h = rect.height();
    let min = rect.min;

    // Sky gradient bands
    let sky_colors = [
        Color32::from_rgb(45, 30, 75),   // Dusk purple
        Color32::from_rgb(120, 50, 85),  // Crimson magenta
        Color32::from_rgb(210, 95, 70),  // Warm peach/salmon
        Color32::from_rgb(245, 160, 90), // Golden orange
    ];
    let bands = sky_colors.len() as f32;
    for (i, &col) in sky_colors.iter().enumerate() {
        let y0 = min.y + (i as f32 / bands) * (h * 0.65);
        let y1 = min.y + ((i + 1) as f32 / bands) * (h * 0.65);
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(min.x, y0), Pos2::new(min.x + w, y1)),
            CornerRadius::ZERO,
            col,
        );
    }

    // Distant soft mountain ridge
    let ridge_far = vec![
        Pos2::new(min.x, min.y + h * 0.55),
        Pos2::new(min.x + w * 0.2, min.y + h * 0.40),
        Pos2::new(min.x + w * 0.45, min.y + h * 0.48),
        Pos2::new(min.x + w * 0.75, min.y + h * 0.35),
        Pos2::new(min.x + w, min.y + h * 0.50),
        Pos2::new(min.x + w, min.y + h),
        Pos2::new(min.x, min.y + h),
    ];
    painter.add(egui::Shape::convex_polygon(
        ridge_far,
        Color32::from_rgb(85, 45, 75),
        Stroke::NONE,
    ));

    // Mid-ground rugged peaks with warm sun rim
    let ridge_mid = vec![
        Pos2::new(min.x, min.y + h * 0.68),
        Pos2::new(min.x + w * 0.15, min.y + h * 0.46),
        Pos2::new(min.x + w * 0.32, min.y + h * 0.56),
        Pos2::new(min.x + w * 0.55, min.y + h * 0.38), // Highest peak
        Pos2::new(min.x + w * 0.68, min.y + h * 0.52),
        Pos2::new(min.x + w * 0.88, min.y + h * 0.42),
        Pos2::new(min.x + w, min.y + h * 0.60),
        Pos2::new(min.x + w, min.y + h),
        Pos2::new(min.x, min.y + h),
    ];
    painter.add(egui::Shape::convex_polygon(
        ridge_mid,
        Color32::from_rgb(45, 32, 52),
        Stroke::NONE,
    ));

    // Snowcap / sun reflection accents on peak 1
    let peak_sun = vec![
        Pos2::new(min.x + w * 0.55, min.y + h * 0.38),
        Pos2::new(min.x + w * 0.51, min.y + h * 0.44),
        Pos2::new(min.x + w * 0.55, min.y + h * 0.43),
        Pos2::new(min.x + w * 0.60, min.y + h * 0.46),
    ];
    painter.add(egui::Shape::convex_polygon(
        peak_sun,
        Color32::from_rgb(250, 205, 170),
        Stroke::NONE,
    ));

    // Foreground dark silhouette ridge
    let ridge_near = vec![
        Pos2::new(min.x, min.y + h * 0.80),
        Pos2::new(min.x + w * 0.28, min.y + h * 0.65),
        Pos2::new(min.x + w * 0.58, min.y + h * 0.72),
        Pos2::new(min.x + w * 0.82, min.y + h * 0.60),
        Pos2::new(min.x + w, min.y + h * 0.75),
        Pos2::new(min.x + w, min.y + h),
        Pos2::new(min.x, min.y + h),
    ];
    painter.add(egui::Shape::convex_polygon(
        ridge_near,
        Color32::from_rgb(22, 20, 30),
        Stroke::NONE,
    ));
}

/// Concert_Footage: Concert stage with lights and silhouettes
fn paint_concert_stage(painter: &egui::Painter, rect: Rect) {
    let w = rect.width();
    let h = rect.height();
    let min = rect.min;

    // Dark auditorium backdrop
    painter.rect_filled(rect, CornerRadius::ZERO, Color32::from_rgb(14, 12, 22));

    // Radiant spotlight beams
    let center = Pos2::new(min.x + w * 0.5, min.y + h * 0.28);

    let beam_left = vec![
        center,
        Pos2::new(min.x + w * 0.05, min.y + h * 0.85),
        Pos2::new(min.x + w * 0.25, min.y + h * 0.85),
    ];
    painter.add(egui::Shape::convex_polygon(
        beam_left,
        Color32::from_rgba_premultiplied(220, 60, 160, 80),
        Stroke::NONE,
    ));

    let beam_center = vec![
        center,
        Pos2::new(min.x + w * 0.38, min.y + h * 0.85),
        Pos2::new(min.x + w * 0.62, min.y + h * 0.85),
    ];
    painter.add(egui::Shape::convex_polygon(
        beam_center,
        Color32::from_rgba_premultiplied(255, 200, 90, 110),
        Stroke::NONE,
    ));

    let beam_right = vec![
        center,
        Pos2::new(min.x + w * 0.75, min.y + h * 0.85),
        Pos2::new(min.x + w * 0.95, min.y + h * 0.85),
    ];
    painter.add(egui::Shape::convex_polygon(
        beam_right,
        Color32::from_rgba_premultiplied(70, 140, 255, 90),
        Stroke::NONE,
    ));

    // Stage wash glow circle
    painter.circle_filled(
        center,
        h * 0.22,
        Color32::from_rgba_premultiplied(255, 180, 80, 70),
    );
    painter.circle_filled(center, h * 0.10, Color32::from_rgb(255, 240, 200));

    // Foreground crowd silhouette dots/heads
    let crowd_y = min.y + h * 0.82;
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(min.x, crowd_y), Pos2::new(min.x + w, min.y + h)),
        CornerRadius::ZERO,
        Color32::from_rgb(8, 8, 12),
    );

    let head_count = 14;
    for i in 0..head_count {
        let x = min.x + (i as f32 + 0.5) * (w / head_count as f32);
        let head_r = (w / head_count as f32) * 0.38;
        let y_offset = if i % 2 == 0 {
            -head_r * 0.5
        } else {
            -head_r * 0.2
        };
        painter.circle_filled(
            Pos2::new(x, crowd_y + y_offset),
            head_r,
            Color32::from_rgb(8, 8, 12),
        );
    }
}

/// Drone_Shot_4K: Ocean wave barrel
fn paint_ocean_wave(painter: &egui::Painter, rect: Rect) {
    let w = rect.width();
    let h = rect.height();
    let min = rect.min;

    // Deep ocean blue
    painter.rect_filled(rect, CornerRadius::ZERO, Color32::from_rgb(10, 32, 58));

    // Wave arc turquoise gradient
    let wave_body = vec![
        Pos2::new(min.x, min.y + h * 0.25),
        Pos2::new(min.x + w * 0.40, min.y + h * 0.32),
        Pos2::new(min.x + w * 0.70, min.y + h * 0.55),
        Pos2::new(min.x + w * 0.85, min.y + h * 0.80),
        Pos2::new(min.x + w, min.y + h * 0.90),
        Pos2::new(min.x + w, min.y + h),
        Pos2::new(min.x, min.y + h),
    ];
    painter.add(egui::Shape::convex_polygon(
        wave_body,
        Color32::from_rgb(18, 95, 125),
        Stroke::NONE,
    ));

    // Bright wave face
    let wave_face = vec![
        Pos2::new(min.x, min.y + h * 0.42),
        Pos2::new(min.x + w * 0.35, min.y + h * 0.48),
        Pos2::new(min.x + w * 0.65, min.y + h * 0.68),
        Pos2::new(min.x + w * 0.80, min.y + h),
        Pos2::new(min.x, min.y + h),
    ];
    painter.add(egui::Shape::convex_polygon(
        wave_face,
        Color32::from_rgb(32, 160, 185),
        Stroke::NONE,
    ));

    // Foamy lip / white water curl
    let foam = vec![
        Pos2::new(min.x + w * 0.25, min.y + h * 0.34),
        Pos2::new(min.x + w * 0.55, min.y + h * 0.45),
        Pos2::new(min.x + w * 0.62, min.y + h * 0.55),
        Pos2::new(min.x + w * 0.52, min.y + h * 0.52),
        Pos2::new(min.x + w * 0.32, min.y + h * 0.40),
    ];
    painter.add(egui::Shape::convex_polygon(
        foam,
        Color32::from_rgb(230, 245, 255),
        Stroke::NONE,
    ));
}

/// Interview_A7S3: Studio portrait with bokeh
fn paint_interview_portrait(painter: &egui::Painter, rect: Rect) {
    let w = rect.width();
    let h = rect.height();
    let min = rect.min;

    // Dark moody studio backdrop
    painter.rect_filled(rect, CornerRadius::ZERO, Color32::from_rgb(22, 24, 30));

    // Warm bokeh circles in background
    let bokehs = [
        (
            Pos2::new(min.x + w * 0.2, min.y + h * 0.3),
            h * 0.16,
            Color32::from_rgba_premultiplied(220, 160, 90, 35),
        ),
        (
            Pos2::new(min.x + w * 0.8, min.y + h * 0.25),
            h * 0.22,
            Color32::from_rgba_premultiplied(180, 110, 60, 40),
        ),
        (
            Pos2::new(min.x + w * 0.75, min.y + h * 0.65),
            h * 0.14,
            Color32::from_rgba_premultiplied(240, 190, 110, 30),
        ),
    ];
    for (pos, radius, col) in bokehs {
        painter.circle_filled(pos, radius, col);
    }

    // Centered subject silhouette / bust
    let center_x = min.x + w * 0.5;
    let head_y = min.y + h * 0.42;
    let head_r = h * 0.22;

    // Head
    painter.circle_filled(
        Pos2::new(center_x, head_y),
        head_r,
        Color32::from_rgb(180, 140, 125),
    );

    // Hair / rim shadow
    painter.circle_filled(
        Pos2::new(center_x, head_y - head_r * 0.2),
        head_r * 0.95,
        Color32::from_rgb(60, 42, 35),
    );

    // Shoulders
    let shoulders = vec![
        Pos2::new(center_x - w * 0.30, min.y + h),
        Pos2::new(center_x - w * 0.22, min.y + h * 0.72),
        Pos2::new(center_x, min.y + h * 0.64),
        Pos2::new(center_x + w * 0.22, min.y + h * 0.72),
        Pos2::new(center_x + w * 0.30, min.y + h),
    ];
    painter.add(egui::Shape::convex_polygon(
        shoulders,
        Color32::from_rgb(38, 44, 56),
        Stroke::NONE,
    ));

    // Rim light on shoulder edge
    painter.line_segment(
        [
            Pos2::new(center_x + w * 0.22, min.y + h * 0.72),
            Pos2::new(center_x + w * 0.30, min.y + h),
        ],
        Stroke::new(1.5, Color32::from_rgb(255, 205, 140)),
    );
}

/// City_Timelapse: Golden hour skyline
fn paint_city_skyline(painter: &egui::Painter, rect: Rect) {
    let w = rect.width();
    let h = rect.height();
    let min = rect.min;

    // Amber dusk sky
    let sky_colors = [
        Color32::from_rgb(32, 28, 55),
        Color32::from_rgb(140, 65, 55),
        Color32::from_rgb(225, 130, 60),
    ];
    for (i, &col) in sky_colors.iter().enumerate() {
        let y0 = min.y + (i as f32 / 3.0) * (h * 0.65);
        let y1 = min.y + ((i + 1) as f32 / 3.0) * (h * 0.65);
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(min.x, y0), Pos2::new(min.x + w, y1)),
            CornerRadius::ZERO,
            col,
        );
    }

    // Distant building silhouettes
    let buildings = [
        (0.05, 0.12, 0.40),
        (0.20, 0.10, 0.32),
        (0.33, 0.14, 0.26), // Spire tower
        (0.50, 0.11, 0.36),
        (0.64, 0.13, 0.30),
        (0.80, 0.15, 0.42),
    ];
    for (rx, rw, rh) in buildings {
        let bx0 = min.x + rx * w;
        let bx1 = bx0 + rw * w;
        let by0 = min.y + rh * h;
        let by1 = min.y + h;
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(bx0, by0), Pos2::new(bx1, by1)),
            CornerRadius::ZERO,
            Color32::from_rgb(24, 20, 32),
        );
    }

    // Water baseline reflection
    let water_y = min.y + h * 0.82;
    painter.rect_filled(
        Rect::from_min_max(Pos2::new(min.x, water_y), Pos2::new(min.x + w, min.y + h)),
        CornerRadius::ZERO,
        Color32::from_rgb(18, 16, 24),
    );
    // Warm reflection shimmer
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(min.x + w * 0.25, water_y),
            Pos2::new(min.x + w * 0.55, water_y + h * 0.08),
        ),
        CornerRadius::ZERO,
        Color32::from_rgba_premultiplied(220, 120, 60, 60),
    );
}

/// Underwater_01: Deep blue ocean with light rays
fn paint_underwater(painter: &egui::Painter, rect: Rect) {
    let w = rect.width();
    let h = rect.height();
    let min = rect.min;

    // Deep cyan-blue ocean gradient
    let ocean_colors = [
        Color32::from_rgb(15, 95, 135),
        Color32::from_rgb(10, 55, 95),
        Color32::from_rgb(6, 28, 55),
    ];
    for (i, &col) in ocean_colors.iter().enumerate() {
        let y0 = min.y + (i as f32 / 3.0) * h;
        let y1 = min.y + ((i + 1) as f32 / 3.0) * h;
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(min.x, y0), Pos2::new(min.x + w, y1)),
            CornerRadius::ZERO,
            col,
        );
    }

    // Sunbeams filtering from top
    let sunbeam = vec![
        Pos2::new(min.x + w * 0.35, min.y),
        Pos2::new(min.x + w * 0.48, min.y),
        Pos2::new(min.x + w * 0.65, min.y + h * 0.75),
        Pos2::new(min.x + w * 0.45, min.y + h * 0.75),
    ];
    painter.add(egui::Shape::convex_polygon(
        sunbeam,
        Color32::from_rgba_premultiplied(140, 225, 255, 45),
        Stroke::NONE,
    ));

    // Marine creature silhouette (manta ray / turtle)
    let center = Pos2::new(min.x + w * 0.48, min.y + h * 0.58);
    let wings = vec![
        Pos2::new(center.x - w * 0.18, center.y - h * 0.05),
        Pos2::new(center.x, center.y - h * 0.08),
        Pos2::new(center.x + w * 0.18, center.y - h * 0.05),
        Pos2::new(center.x + w * 0.06, center.y + h * 0.06),
        Pos2::new(center.x - w * 0.06, center.y + h * 0.06),
    ];
    painter.add(egui::Shape::convex_polygon(
        wings,
        Color32::from_rgb(6, 42, 65),
        Stroke::NONE,
    ));
}

/// Fallback generic media thumbnail
fn paint_generic_media(painter: &egui::Painter, rect: Rect, _name: &str) {
    painter.rect_filled(rect, CornerRadius::same(3), Color32::from_rgb(24, 28, 36));
    let center = rect.center();
    painter.circle_stroke(
        center,
        rect.height() * 0.22,
        Stroke::new(1.5, Color32::from_rgb(70, 85, 110)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fit_aspect_rect_strictly_16_9() {
        // Height-constrained scenario (e.g. wide panel)
        let bounds_wide = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(800.0, 300.0));
        let fitted_wide = fit_aspect_rect(bounds_wide, 16.0 / 9.0);
        let ratio_wide = fitted_wide.width() / fitted_wide.height();
        assert!((ratio_wide - (16.0 / 9.0)).abs() < 1e-4);
        assert_eq!(fitted_wide.height(), 300.0);
        assert!((fitted_wide.width() - (300.0 * 16.0 / 9.0)).abs() < 1e-3);
        assert!((fitted_wide.center().x - bounds_wide.center().x).abs() < 1e-3);

        // Width-constrained scenario (e.g. tall panel)
        let bounds_tall = Rect::from_min_size(Pos2::new(50.0, 50.0), Vec2::new(320.0, 600.0));
        let fitted_tall = fit_aspect_rect(bounds_tall, 16.0 / 9.0);
        let ratio_tall = fitted_tall.width() / fitted_tall.height();
        assert!((ratio_tall - (16.0 / 9.0)).abs() < 1e-4);
        assert_eq!(fitted_tall.width(), 320.0);
        assert_eq!(fitted_tall.height(), 320.0 / (16.0 / 9.0));
        assert!((fitted_tall.center().y - bounds_tall.center().y).abs() < 1e-3);
    }

    #[test]
    fn test_calculate_cover_uv_preserves_geometry() {
        let target_16_9 = Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 360.0));

        // Texture matching target aspect ratio -> full UVs
        let uv_exact = calculate_cover_uv([1920, 1080], target_16_9);
        assert_eq!(uv_exact.min, Pos2::new(0.0, 0.0));
        assert_eq!(uv_exact.max, Pos2::new(1.0, 1.0));

        // Texture wider than target (e.g. 21:9 in 16:9) -> symmetric horizontal crop
        let uv_wide = calculate_cover_uv([2100, 900], target_16_9);
        assert_eq!(uv_wide.min.y, 0.0);
        assert_eq!(uv_wide.max.y, 1.0);
        assert!(uv_wide.min.x > 0.0);
        assert!((uv_wide.min.x - (1.0 - uv_wide.max.x)).abs() < 1e-5);

        // Texture taller than target (e.g. 4:3 in 16:9) -> symmetric vertical crop
        let uv_tall = calculate_cover_uv([1200, 900], target_16_9);
        assert_eq!(uv_tall.min.x, 0.0);
        assert_eq!(uv_tall.max.x, 1.0);
        assert!(uv_tall.min.y > 0.0);
        assert!((uv_tall.min.y - (1.0 - uv_tall.max.y)).abs() < 1e-5);
    }

    #[test]
    fn test_embedded_clip_assets_exact_16_9() {
        let clip_bytes = [
            ("mountain", CLIP_MOUNTAIN_BYTES),
            ("concert", CLIP_CONCERT_BYTES),
            ("drone", CLIP_DRONE_BYTES),
            ("interview", CLIP_INTERVIEW_BYTES),
            ("city", CLIP_CITY_BYTES),
            ("underwater", CLIP_UNDERWATER_BYTES),
        ];

        for (name, bytes) in clip_bytes {
            let img = image::load_from_memory(bytes)
                .unwrap_or_else(|e| panic!("Failed to load {name}: {e}"));
            let w = img.width() as f32;
            let h = img.height() as f32;
            let aspect = w / h;
            assert!(
                (aspect - (16.0 / 9.0)).abs() < 0.01,
                "Clip {name} aspect ratio is {aspect:.4} (w={w}, h={h}), expected ~1.7778",
            );
        }
    }
}
