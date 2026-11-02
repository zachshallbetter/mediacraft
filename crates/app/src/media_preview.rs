use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread;
use std::time::{Duration, Instant};

use egui::{Color32, ColorImage, Context, CornerRadius, Pos2, Rect, TextureHandle, TextureOptions};
use image::{ImageReader, Limits};
use mediacraft_catalog::{MediaAsset, MediaKind};

const MAX_IMAGE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;
const MAX_AUDIO_BYTES: usize = 512 * 1024;
const MAX_PREVIEW_EDGE: u32 = 1280;
const MAX_SOURCE_EDGE: u32 = 8192;
const MAX_CACHE_ENTRIES: usize = 256;
const FFMPEG_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PreviewKey {
    path: PathBuf,
    seek_ms: u32,
}

struct PreviewRequest {
    key: PreviewKey,
    kind: MediaKind,
}

pub struct MediaPreviewCache {
    requests: SyncSender<PreviewRequest>,
    results: Receiver<(PreviewKey, Option<ColorImage>)>,
    pending: HashSet<PreviewKey>,
    failed: HashSet<PreviewKey>,
    textures: HashMap<PreviewKey, TextureHandle>,
    insertion_order: VecDeque<PreviewKey>,
}

impl Default for MediaPreviewCache {
    fn default() -> Self {
        let (request_tx, request_rx) = mpsc::sync_channel::<PreviewRequest>(64);
        let (result_tx, result_rx) = mpsc::sync_channel::<(PreviewKey, Option<ColorImage>)>(8);

        let _ = thread::Builder::new()
            .name("mediacraft-preview-decoder".to_string())
            .spawn(move || {
                while let Ok(request) = request_rx.recv() {
                    let image =
                        decode_preview(&request.key.path, request.kind, request.key.seek_ms);
                    if result_tx.send((request.key, image)).is_err() {
                        break;
                    }
                }
            });

        Self {
            requests: request_tx,
            results: result_rx,
            pending: HashSet::new(),
            failed: HashSet::new(),
            textures: HashMap::new(),
            insertion_order: VecDeque::new(),
        }
    }
}

impl MediaPreviewCache {
    pub fn texture_for(
        &mut self,
        ctx: &Context,
        asset: &MediaAsset,
        seek_seconds: f32,
    ) -> Option<TextureHandle> {
        while let Ok((key, image)) = self.results.try_recv() {
            self.pending.remove(&key);
            if let Some(image) = image {
                while self.textures.len() >= MAX_CACHE_ENTRIES {
                    if let Some(oldest) = self.insertion_order.pop_front() {
                        self.textures.remove(&oldest);
                    } else {
                        break;
                    }
                }
                let texture = ctx.load_texture(
                    format!("media-preview:{}", key.path.display()),
                    image,
                    TextureOptions::LINEAR,
                );
                self.insertion_order.push_back(key.clone());
                self.textures.insert(key, texture);
            } else {
                self.failed.insert(key);
            }
        }

        if !supports_preview(asset.kind) {
            return None;
        }
        let key = preview_key(asset, seek_seconds);

        if let Some(texture) = self.textures.get(&key) {
            return Some(texture.clone());
        }
        if self.failed.contains(&key) {
            return None;
        }
        if !self.pending.contains(&key) {
            match self.requests.try_send(PreviewRequest {
                key: key.clone(),
                kind: asset.kind,
            }) {
                Ok(()) => {
                    self.pending.insert(key);
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
                Err(TrySendError::Full(_)) => {
                    ctx.request_repaint_after(Duration::from_millis(250));
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.failed.insert(key);
                }
            }
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        None
    }

    fn failed_for(&self, asset: &MediaAsset, seek_seconds: f32) -> bool {
        self.failed.contains(&preview_key(asset, seek_seconds))
    }
}

fn preview_key(asset: &MediaAsset, seek_seconds: f32) -> PreviewKey {
    let seek_ms = if asset.kind == MediaKind::Video {
        let ms = (seek_seconds.max(0.0) * 1000.0).round() as u32;
        (ms / 250) * 250
    } else {
        0
    };
    PreviewKey {
        path: asset.path.clone(),
        seek_ms,
    }
}

fn supports_preview(kind: MediaKind) -> bool {
    matches!(
        kind,
        MediaKind::Image | MediaKind::Video | MediaKind::Audio | MediaKind::RawDng
    )
}

pub fn paint_asset_preview(
    previews: &mut MediaPreviewCache,
    ctx: &Context,
    painter: &egui::Painter,
    rect: Rect,
    asset: &MediaAsset,
    seek_seconds: f32,
) -> bool {
    let corner = CornerRadius::same(3);
    painter.rect_filled(rect, corner, Color32::from_rgb(19, 23, 29));

    if let Some(texture) = previews.texture_for(ctx, asset, seek_seconds) {
        painter.image(
            texture.id(),
            rect,
            cover_uv(texture.size(), rect),
            Color32::WHITE,
        );
        painter.rect_stroke(
            rect,
            corner,
            egui::Stroke::new(1.0, Color32::from_rgba_premultiplied(255, 255, 255, 35)),
            egui::StrokeKind::Inside,
        );
        return true;
    }

    let (glyph, label) = match asset.kind {
        MediaKind::Video => ("▶", "VIDEO FRAME"),
        MediaKind::Audio => ("♫", "AUDIO"),
        MediaKind::Image | MediaKind::RawDng => ("▧", "IMAGE PREVIEW"),
        MediaKind::ImageSequence => ("▦", "IMAGE SEQUENCE"),
        MediaKind::ProjectFile => ("▤", "PROJECT FILE"),
        MediaKind::Document => ("▤", "DOCUMENT"),
        MediaKind::Archive => ("▣", "ARCHIVE"),
        MediaKind::Other => ("•", "MEDIA FILE"),
    };
    let preview_status = if !asset.path.is_file() {
        "SOURCE OFFLINE"
    } else if !supports_preview(asset.kind) {
        "PREVIEW NOT SUPPORTED"
    } else if previews.failed_for(asset, seek_seconds) {
        "PREVIEW UNAVAILABLE"
    } else {
        label
    };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional((rect.height() * 0.34).clamp(18.0, 42.0)),
        Color32::from_rgb(105, 125, 148),
    );
    painter.text(
        Pos2::new(rect.center().x, rect.max.y - 9.0),
        egui::Align2::CENTER_BOTTOM,
        preview_status,
        egui::FontId::proportional(8.0),
        Color32::from_rgb(130, 145, 165),
    );
    painter.rect_stroke(
        rect,
        corner,
        egui::Stroke::new(1.0, Color32::from_rgb(45, 52, 65)),
        egui::StrokeKind::Inside,
    );
    false
}

fn cover_uv(texture_size: [usize; 2], target: Rect) -> Rect {
    let texture_aspect = texture_size[0] as f32 / texture_size[1].max(1) as f32;
    let target_aspect = target.width() / target.height().max(1.0);
    if texture_aspect > target_aspect {
        let visible = target_aspect / texture_aspect;
        let margin = (1.0 - visible) * 0.5;
        Rect::from_min_max(Pos2::new(margin, 0.0), Pos2::new(1.0 - margin, 1.0))
    } else {
        let visible = texture_aspect / target_aspect;
        let margin = (1.0 - visible) * 0.5;
        Rect::from_min_max(Pos2::new(0.0, margin), Pos2::new(1.0, 1.0 - margin))
    }
}

fn decode_preview(path: &Path, kind: MediaKind, seek_ms: u32) -> Option<ColorImage> {
    match kind {
        MediaKind::Image => decode_image_file(path).or_else(|| decode_ffmpeg_still(path)),
        MediaKind::Video => decode_video_frame(path, seek_ms),
        MediaKind::Audio => decode_audio_waveform(path),
        MediaKind::RawDng => decode_ffmpeg_still(path),
        _ => None,
    }
}

fn decode_image_file(path: &Path) -> Option<ColorImage> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_IMAGE_BYTES {
        return None;
    }

    let reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    let (width, height) = reader.into_dimensions().ok()?;
    if width > MAX_SOURCE_EDGE || height > MAX_SOURCE_EDGE {
        return None;
    }
    let mut reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
    reader.limits(preview_limits());
    to_color_image(reader.decode().ok()?)
}

fn decode_video_frame(path: &Path, seek_ms: u32) -> Option<ColorImage> {
    if !is_supported_video(path) || !path.is_file() {
        return None;
    }

    let seek_seconds = format!("{:.3}", seek_ms as f32 / 1000.0);
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-max_alloc",
            "67108864",
            "-protocol_whitelist",
            "file,pipe",
            "-ss",
            &seek_seconds,
            "-i",
        ])
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=1280:1280:force_original_aspect_ratio=decrease",
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "pipe:1",
        ]);
    let bytes = run_ffmpeg(command, MAX_FRAME_BYTES)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    reader.limits(preview_limits());
    to_color_image(reader.decode().ok()?)
}

fn decode_ffmpeg_still(path: &Path) -> Option<ColorImage> {
    if !is_supported_ffmpeg_still(path) || !path.is_file() {
        return None;
    }
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-threads",
            "1",
            "-max_alloc",
            "67108864",
            "-protocol_whitelist",
            "file,pipe",
            "-i",
        ])
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-vf",
            "scale=1280:1280:force_original_aspect_ratio=decrease",
            "-f",
            "image2pipe",
            "-vcodec",
            "png",
            "pipe:1",
        ]);
    let bytes = run_ffmpeg(command, MAX_FRAME_BYTES)?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    reader.limits(preview_limits());
    to_color_image(reader.decode().ok()?)
}

fn decode_audio_waveform(path: &Path) -> Option<ColorImage> {
    if !is_supported_audio(path) || !path.is_file() {
        return None;
    }
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-threads",
            "1",
            "-max_alloc",
            "67108864",
            "-protocol_whitelist",
            "file,pipe",
            "-i",
        ])
        .arg(path)
        .args([
            "-map", "0:a:0", "-t", "12", "-ac", "1", "-ar", "4000", "-f", "s16le", "pipe:1",
        ]);
    let samples = run_ffmpeg(command, MAX_AUDIO_BYTES)?;
    waveform_image(&samples)
}

fn run_ffmpeg(mut command: Command, max_output_bytes: usize) -> Option<Vec<u8>> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = match thread::Builder::new()
        .name("mediacraft-preview-output".to_string())
        .spawn(move || {
            let mut output = Vec::with_capacity(512 * 1024);
            let mut limited = (&mut stdout).take((max_output_bytes + 1) as u64);
            limited.read_to_end(&mut output).ok()?;
            (output.len() <= max_output_bytes).then_some(output)
        }) {
        Ok(reader) => reader,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };

    let deadline = Instant::now() + FFMPEG_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let bytes = reader.join().ok().flatten()?;
    if !status?.success() || bytes.is_empty() {
        return None;
    }
    Some(bytes)
}

fn waveform_image(bytes: &[u8]) -> Option<ColorImage> {
    let sample_count = bytes.len() / 2;
    if sample_count == 0 {
        return None;
    }

    const WIDTH: usize = 640;
    const HEIGHT: usize = 180;
    let center = HEIGHT / 2;
    let mut image = ColorImage::filled([WIDTH, HEIGHT], Color32::from_rgb(16, 20, 26));
    for x in 0..WIDTH {
        let start = x * sample_count / WIDTH;
        let end = ((x + 1) * sample_count / WIDTH)
            .max(start + 1)
            .min(sample_count);
        let peak = (start..end)
            .map(|sample| {
                let offset = sample * 2;
                i16::from_le_bytes([bytes[offset], bytes[offset + 1]]).unsigned_abs() as usize
            })
            .max()
            .unwrap_or_default();
        let amplitude = (peak * (center - 3) / i16::MAX as usize).max(1);
        for y in center.saturating_sub(amplitude)..=(center + amplitude).min(HEIGHT - 1) {
            image.pixels[y * WIDTH + x] = Color32::from_rgb(33, 205, 198);
        }
    }
    Some(image)
}

fn preview_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_EDGE);
    limits.max_image_height = Some(MAX_SOURCE_EDGE);
    limits.max_alloc = Some(256 * 1024 * 1024);
    limits
}

fn to_color_image(image: image::DynamicImage) -> Option<ColorImage> {
    let image = image
        .thumbnail(MAX_PREVIEW_EDGE, MAX_PREVIEW_EDGE)
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    (!image.is_empty()).then(|| ColorImage::from_rgba_unmultiplied(size, image.as_raw()))
}

fn is_supported_video(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "mov" | "mp4" | "m4v" | "mkv" | "avi" | "mxf" | "webm" | "mts" | "m2ts" | "mpeg" | "mpg"
    )
}

fn is_supported_audio(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "wav" | "aif" | "aiff" | "m4a" | "aac" | "flac" | "mp3" | "ogg" | "opus"
    )
}

fn is_supported_ffmpeg_still(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "heic" | "heif" | "avif" | "dng" | "arw" | "cr3" | "nef"
    )
}
