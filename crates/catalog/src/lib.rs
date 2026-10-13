//! MediaCraft Asset Catalog & Derivative Graph Engine
//!
//! Provides filesystem-native asset indexing, technical stream metadata models,
//! faceted searching, and immutable derivative relationship tracking (originals -> proxies/deliveries).

pub mod batch_rename;
pub mod duplicates;
pub mod scanner;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// High-level categorization of media assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaKind {
    Video,
    Audio,
    Image,
    RawDng,
    ImageSequence,
    ProjectFile,
    Other,
}

impl MediaKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Image => "Image",
            Self::RawDng => "RAW DNG",
            Self::ImageSequence => "Image Sequence",
            Self::ProjectFile => "Project File",
            Self::Other => "Other",
        }
    }

    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "mov" | "mp4" | "m4v" | "mkv" | "avi" | "mxf" => Self::Video,
            "wav" | "aif" | "aiff" | "m4a" | "aac" | "flac" | "mp3" => Self::Audio,
            "png" | "jpg" | "jpeg" | "webp" | "tif" | "tiff" | "exr" => Self::Image,
            "dng" | "arw" | "cr3" | "nef" | "braw" | "r3d" => Self::RawDng,
            "fcp" | "prproj" | "aep" | "cpr" | "ptx" => Self::ProjectFile,
            _ => Self::Other,
        }
    }
}

/// Technical video stream metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoStreamInfo {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub color_space: String,
    pub hdr: bool,
    pub bit_depth: u8,
}

/// Technical audio stream metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioStreamInfo {
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u8,
    pub bit_depth: u8,
    pub bitrate_kbps: u32,
}

/// An indexed media asset in the MediaCraft workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaAsset {
    pub id: String,
    pub path: PathBuf,
    pub filename: String,
    pub file_size_bytes: u64,
    pub kind: MediaKind,
    pub duration_secs: Option<f32>,
    pub timecode_start: String,
    pub video: Option<VideoStreamInfo>,
    pub audio: Option<AudioStreamInfo>,
    pub camera_model: Option<String>,
    pub date_created: String,
    pub geo_location: Option<String>,
    pub rating: u8,
    pub tags: Vec<String>,
    pub has_proxy: bool,
    pub needs_transcode: bool,
}

impl MediaAsset {
    pub fn formatted_size(&self) -> String {
        let bytes = self.file_size_bytes;
        if bytes >= 1_000_000_000 {
            format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
        } else if bytes >= 1_000_000 {
            format!("{:.1} MB", bytes as f64 / 1_000_000.0)
        } else if bytes >= 1_000 {
            format!("{:.1} KB", bytes as f64 / 1_000.0)
        } else {
            format!("{bytes} B")
        }
    }

    pub fn formatted_duration(&self) -> String {
        match self.duration_secs {
            Some(secs) => {
                let total_s = secs.round() as u32;
                let m = total_s / 60;
                let s = total_s % 60;
                format!("{m:02}:{s:02}")
            }
            None => "--:--".to_string(),
        }
    }
}

/// Operation types supported on assets in MediaCraft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperationKind {
    Convert,
    Optimize,
    Extract,
    Analyze,
    Proxy,
    ImageSequence,
}

impl OperationKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Convert => "Convert",
            Self::Optimize => "Optimize",
            Self::Extract => "Extract",
            Self::Analyze => "Analyze",
            Self::Proxy => "Proxy",
            Self::ImageSequence => "Image Sequence",
        }
    }
}

/// Record of derivation from an original asset to a created derivative.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivativeRecord {
    pub derivative_id: String,
    pub path: PathBuf,
    pub operation: OperationKind,
    pub preset_name: String,
    pub date_generated: String,
    pub size_bytes: u64,
    pub verified: bool,
}

/// Tracks the relationship tree between originals, proxies, and output deliverables.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DerivativeGraph {
    /// Maps source_asset_id -> Vec<DerivativeRecord>
    pub edges: HashMap<String, Vec<DerivativeRecord>>,
}

impl DerivativeGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_derivative(&mut self, source_id: &str, record: DerivativeRecord) {
        self.edges
            .entry(source_id.to_string())
            .or_default()
            .push(record);
    }

    pub fn get_derivatives(&self, source_id: &str) -> &[DerivativeRecord] {
        self.edges
            .get(source_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

/// Smart collection dynamic predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SmartCollection {
    AllMedia,
    ClientDeliverables,
    EditingProxies,
    NeedsTranscode,
    FourKOriginals,
    AudioForReview,
    ImageSequences,
}

impl SmartCollection {
    pub fn label(&self) -> &'static str {
        match self {
            Self::AllMedia => "All Media",
            Self::ClientDeliverables => "Client Deliverables",
            Self::EditingProxies => "Editing Proxies",
            Self::NeedsTranscode => "Needs Transcode",
            Self::FourKOriginals => "4K Originals",
            Self::AudioForReview => "Audio for Review",
            Self::ImageSequences => "Image Sequences",
        }
    }

    pub fn matches(&self, asset: &MediaAsset) -> bool {
        match self {
            Self::AllMedia => true,
            Self::ClientDeliverables => asset.tags.iter().any(|t| t == "deliverable"),
            Self::EditingProxies => asset.has_proxy || asset.tags.iter().any(|t| t == "proxy"),
            Self::NeedsTranscode => asset.needs_transcode,
            Self::FourKOriginals => asset
                .video
                .as_ref()
                .map(|v| v.width >= 3840 || v.height >= 2160)
                .unwrap_or(false),
            Self::AudioForReview => asset.kind == MediaKind::Audio,
            Self::ImageSequences => asset.kind == MediaKind::ImageSequence,
        }
    }
}

/// Media Catalog holding indexed assets and facilitating faceted search.
#[derive(Debug, Clone)]
pub struct MediaCatalog {
    assets: Vec<MediaAsset>,
    graph: DerivativeGraph,
}

impl Default for MediaCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaCatalog {
    pub fn new() -> Self {
        Self {
            assets: Vec::new(),
            graph: DerivativeGraph::new(),
        }
    }

    /// Populates standard mock corpus assets matching the reference designs.
    pub fn with_standard_mock_corpus() -> Self {
        let mut cat = Self::new();

        let mock_assets = vec![
            MediaAsset {
                id: "A001_C001".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/A001_C001.mov"),
                filename: "A001_C001.mov".to_string(),
                file_size_bytes: 1_200_000_000,
                kind: MediaKind::Video,
                duration_secs: Some(12.23),
                timecode_start: "01:00:00:00".to_string(),
                video: Some(VideoStreamInfo {
                    codec: "Apple ProRes 422 HQ".to_string(),
                    width: 3840,
                    height: 2160,
                    fps: 23.976,
                    color_space: "Rec. 709".to_string(),
                    hdr: true,
                    bit_depth: 10,
                }),
                audio: Some(AudioStreamInfo {
                    codec: "PCM 24-bit".to_string(),
                    sample_rate: 48000,
                    channels: 2,
                    bit_depth: 24,
                    bitrate_kbps: 2304,
                }),
                camera_model: Some("ARRI A7S III".to_string()),
                date_created: "2026-07-03 14:23:11".to_string(),
                geo_location: Some("46.8787, -112.0217".to_string()),
                rating: 5,
                tags: vec![
                    "b-roll".to_string(),
                    "master".to_string(),
                    "nature".to_string(),
                ],
                has_proxy: false,
                needs_transcode: true,
            },
            MediaAsset {
                id: "B002_C014".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/B002_C014.mp4"),
                filename: "B002_C014.mp4".to_string(),
                file_size_bytes: 340_000_000,
                kind: MediaKind::Video,
                duration_secs: Some(138.0),
                timecode_start: "00:02:18:00".to_string(),
                video: Some(VideoStreamInfo {
                    codec: "H.264 (High Profile)".to_string(),
                    width: 3840,
                    height: 2160,
                    fps: 29.97,
                    color_space: "Rec. 709".to_string(),
                    hdr: false,
                    bit_depth: 8,
                }),
                audio: Some(AudioStreamInfo {
                    codec: "AAC-LC".to_string(),
                    sample_rate: 48000,
                    channels: 2,
                    bit_depth: 16,
                    bitrate_kbps: 320,
                }),
                camera_model: Some("Sony FX3".to_string()),
                date_created: "2026-07-04 10:11:42".to_string(),
                geo_location: None,
                rating: 4,
                tags: vec!["interview".to_string()],
                has_proxy: true,
                needs_transcode: false,
            },
            MediaAsset {
                id: "C003_C007".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/C003_C007.mov"),
                filename: "C003_C007.mov".to_string(),
                file_size_bytes: 840_000_000,
                kind: MediaKind::Video,
                duration_secs: Some(28.0),
                timecode_start: "00:00:28:00".to_string(),
                video: Some(VideoStreamInfo {
                    codec: "ProRes 422".to_string(),
                    width: 3840,
                    height: 2160,
                    fps: 24.0,
                    color_space: "Rec. 709".to_string(),
                    hdr: false,
                    bit_depth: 10,
                }),
                audio: None,
                camera_model: Some("Blackmagic Cinema 6K".to_string()),
                date_created: "2026-07-04 16:45:00".to_string(),
                geo_location: None,
                rating: 5,
                tags: vec!["hero".to_string()],
                has_proxy: false,
                needs_transcode: true,
            },
            MediaAsset {
                id: "IMG_8342".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/IMG_8342.dng"),
                filename: "IMG_8342.dng".to_string(),
                file_size_bytes: 45_200_000,
                kind: MediaKind::RawDng,
                duration_secs: None,
                timecode_start: "00:00:00:00".to_string(),
                video: None,
                audio: None,
                camera_model: Some("Canon EOS R5".to_string()),
                date_created: "2026-07-05 09:20:14".to_string(),
                geo_location: None,
                rating: 4,
                tags: vec!["raw".to_string(), "photo".to_string()],
                has_proxy: false,
                needs_transcode: false,
            },
            MediaAsset {
                id: "Surf_001".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/Surf_001.mov"),
                filename: "Surf_001.mov".to_string(),
                file_size_bytes: 520_000_000,
                kind: MediaKind::Video,
                duration_secs: Some(14.0),
                timecode_start: "00:00:14:00".to_string(),
                video: Some(VideoStreamInfo {
                    codec: "DNxHR HQ".to_string(),
                    width: 3840,
                    height: 2160,
                    fps: 59.94,
                    color_space: "Rec. 709".to_string(),
                    hdr: false,
                    bit_depth: 8,
                }),
                audio: None,
                camera_model: Some("RED V-Raptor".to_string()),
                date_created: "2026-07-05 12:00:00".to_string(),
                geo_location: None,
                rating: 5,
                tags: vec!["slowmo".to_string()],
                has_proxy: false,
                needs_transcode: true,
            },
            MediaAsset {
                id: "Podcast_Interview".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/Podcast_Interview.wav"),
                filename: "Podcast_Interview.wav".to_string(),
                file_size_bytes: 460_000_000,
                kind: MediaKind::Audio,
                duration_secs: Some(2531.0),
                timecode_start: "00:00:00:00".to_string(),
                video: None,
                audio: Some(AudioStreamInfo {
                    codec: "PCM Broadcast WAV".to_string(),
                    sample_rate: 48000,
                    channels: 2,
                    bit_depth: 24,
                    bitrate_kbps: 2304,
                }),
                camera_model: None,
                date_created: "2026-07-06 14:00:00".to_string(),
                geo_location: None,
                rating: 4,
                tags: vec!["dialogue".to_string(), "audio".to_string()],
                has_proxy: false,
                needs_transcode: true,
            },
            MediaAsset {
                id: "Motion_Graphics".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/Motion_Graphics.mov"),
                filename: "Motion_Graphics.mov".to_string(),
                file_size_bytes: 180_000_000,
                kind: MediaKind::Video,
                duration_secs: Some(10.0),
                timecode_start: "00:00:10:00".to_string(),
                video: Some(VideoStreamInfo {
                    codec: "Apple ProRes 4444 (Alpha)".to_string(),
                    width: 1920,
                    height: 1080,
                    fps: 24.0,
                    color_space: "Rec. 709".to_string(),
                    hdr: false,
                    bit_depth: 12,
                }),
                audio: None,
                camera_model: Some("EffectCraft Render".to_string()),
                date_created: "2026-07-06 18:30:00".to_string(),
                geo_location: None,
                rating: 5,
                tags: vec!["vfx".to_string(), "alpha".to_string()],
                has_proxy: false,
                needs_transcode: false,
            },
            MediaAsset {
                id: "Drone_003".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/Drone_003.mp4"),
                filename: "Drone_003.mp4".to_string(),
                file_size_bytes: 290_000_000,
                kind: MediaKind::Video,
                duration_secs: Some(43.0),
                timecode_start: "00:00:43:00".to_string(),
                video: Some(VideoStreamInfo {
                    codec: "HEVC / H.265".to_string(),
                    width: 3840,
                    height: 2160,
                    fps: 29.97,
                    color_space: "Rec. 709".to_string(),
                    hdr: false,
                    bit_depth: 10,
                }),
                audio: None,
                camera_model: Some("DJI Mavic 3 Pro".to_string()),
                date_created: "2026-07-07 08:15:00".to_string(),
                geo_location: None,
                rating: 4,
                tags: vec!["aerial".to_string()],
                has_proxy: false,
                needs_transcode: false,
            },
            MediaAsset {
                id: "Color_Bars".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/Color_Bars.mov"),
                filename: "Color_Bars.mov".to_string(),
                file_size_bytes: 120_000_000,
                kind: MediaKind::Video,
                duration_secs: Some(10.0),
                timecode_start: "00:00:00:00".to_string(),
                video: Some(VideoStreamInfo {
                    codec: "Apple ProRes 4444".to_string(),
                    width: 1920,
                    height: 1080,
                    fps: 24.0,
                    color_space: "Rec. 709".to_string(),
                    hdr: false,
                    bit_depth: 10,
                }),
                audio: Some(AudioStreamInfo {
                    codec: "1 kHz Sine".to_string(),
                    sample_rate: 48000,
                    channels: 2,
                    bit_depth: 24,
                    bitrate_kbps: 2304,
                }),
                camera_model: Some("Synthetic Generator".to_string()),
                date_created: "2026-07-07 09:00:00".to_string(),
                geo_location: None,
                rating: 5,
                tags: vec!["test".to_string(), "calibration".to_string()],
                has_proxy: false,
                needs_transcode: false,
            },
            MediaAsset {
                id: "Ski_Day_01".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/Ski_Day_01.mp4"),
                filename: "Ski_Day_01.mp4".to_string(),
                file_size_bytes: 380_000_000,
                kind: MediaKind::Video,
                duration_secs: Some(86.0),
                timecode_start: "00:01:26:00".to_string(),
                video: Some(VideoStreamInfo {
                    codec: "H.264".to_string(),
                    width: 3840,
                    height: 2160,
                    fps: 59.94,
                    color_space: "Rec. 709".to_string(),
                    hdr: false,
                    bit_depth: 8,
                }),
                audio: None,
                camera_model: Some("GoPro Hero 12".to_string()),
                date_created: "2026-07-08 11:30:00".to_string(),
                geo_location: None,
                rating: 4,
                tags: vec!["action".to_string()],
                has_proxy: false,
                needs_transcode: false,
            },
            MediaAsset {
                id: "Waterfall_Exr".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/Waterfall.exr"),
                filename: "Waterfall.exr".to_string(),
                file_size_bytes: 16_800_000,
                kind: MediaKind::ImageSequence,
                duration_secs: None,
                timecode_start: "00:00:00:00".to_string(),
                video: None,
                audio: None,
                camera_model: Some("VFX Plate".to_string()),
                date_created: "2026-07-08 15:45:00".to_string(),
                geo_location: None,
                rating: 5,
                tags: vec!["exr".to_string(), "vfx".to_string()],
                has_proxy: false,
                needs_transcode: false,
            },
            MediaAsset {
                id: "Interview_B_Roll".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/Interview_B_Roll.mp4"),
                filename: "Interview_B_Roll.mp4".to_string(),
                file_size_bytes: 92_000_000,
                kind: MediaKind::Video,
                duration_secs: Some(36.0),
                timecode_start: "00:00:36:00".to_string(),
                video: Some(VideoStreamInfo {
                    codec: "H.264".to_string(),
                    width: 1920,
                    height: 1080,
                    fps: 23.976,
                    color_space: "Rec. 709".to_string(),
                    hdr: false,
                    bit_depth: 8,
                }),
                audio: None,
                camera_model: Some("Sony A7IV".to_string()),
                date_created: "2026-07-09 13:10:00".to_string(),
                geo_location: None,
                rating: 3,
                tags: vec!["interview".to_string()],
                has_proxy: false,
                needs_transcode: false,
            },
            MediaAsset {
                id: "Product_Shot".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/Product_Shot.png"),
                filename: "Product_Shot.png".to_string(),
                file_size_bytes: 8_400_000,
                kind: MediaKind::Image,
                duration_secs: None,
                timecode_start: "00:00:00:00".to_string(),
                video: None,
                audio: None,
                camera_model: Some("Studio Rig".to_string()),
                date_created: "2026-07-09 17:00:00".to_string(),
                geo_location: None,
                rating: 5,
                tags: vec!["product".to_string(), "packshot".to_string()],
                has_proxy: false,
                needs_transcode: false,
            },
            MediaAsset {
                id: "Aerial_Utah".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/Aerial_Utah.mov"),
                filename: "Aerial_Utah.mov".to_string(),
                file_size_bytes: 680_000_000,
                kind: MediaKind::Video,
                duration_secs: Some(52.0),
                timecode_start: "00:00:52:00".to_string(),
                video: Some(VideoStreamInfo {
                    codec: "HEVC (Rec. 2020)".to_string(),
                    width: 3840,
                    height: 2160,
                    fps: 24.0,
                    color_space: "Rec. 2020 HLG".to_string(),
                    hdr: true,
                    bit_depth: 10,
                }),
                audio: None,
                camera_model: Some("Inspire 3".to_string()),
                date_created: "2026-07-10 16:20:00".to_string(),
                geo_location: None,
                rating: 5,
                tags: vec!["aerial".to_string(), "hdr".to_string()],
                has_proxy: false,
                needs_transcode: true,
            },
            MediaAsset {
                id: "Car_Commercial".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/Car_Commercial.mov"),
                filename: "Car_Commercial.mov".to_string(),
                file_size_bytes: 750_000_000,
                kind: MediaKind::Video,
                duration_secs: Some(30.0),
                timecode_start: "00:00:30:00".to_string(),
                video: Some(VideoStreamInfo {
                    codec: "ProRes 422".to_string(),
                    width: 3840,
                    height: 2160,
                    fps: 24.0,
                    color_space: "Rec. 709".to_string(),
                    hdr: false,
                    bit_depth: 10,
                }),
                audio: Some(AudioStreamInfo {
                    codec: "PCM Stereo".to_string(),
                    sample_rate: 48000,
                    channels: 2,
                    bit_depth: 24,
                    bitrate_kbps: 2304,
                }),
                camera_model: Some("ARRI Alexa Mini".to_string()),
                date_created: "2026-07-10 19:40:00".to_string(),
                geo_location: None,
                rating: 5,
                tags: vec!["commercial".to_string(), "deliverable".to_string()],
                has_proxy: true,
                needs_transcode: false,
            },
        ];

        for a in mock_assets {
            cat.add_asset(a);
        }

        // Add derivative relationships for A001_C001
        cat.graph.add_derivative(
            "A001_C001",
            DerivativeRecord {
                derivative_id: "A001_C001_proxy".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/A001_C001_proxy.mp4"),
                operation: OperationKind::Proxy,
                preset_name: "Editing Proxy (H.264 1080p 5 Mbps)".to_string(),
                date_generated: "2026-10-12 14:10:00".to_string(),
                size_bytes: 42_000_000,
                verified: true,
            },
        );

        cat.graph.add_derivative(
            "A001_C001",
            DerivativeRecord {
                derivative_id: "A001_C001_web".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/A001_C001_web.mp4"),
                operation: OperationKind::Convert,
                preset_name: "Web Delivery (H.264 1080p 8 Mbps)".to_string(),
                date_generated: "2026-10-12 14:15:00".to_string(),
                size_bytes: 68_000_000,
                verified: true,
            },
        );

        cat.graph.add_derivative(
            "A001_C001",
            DerivativeRecord {
                derivative_id: "A001_C001_audio".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/A001_C001_audio.m4a"),
                operation: OperationKind::Extract,
                preset_name: "Audio Only (AAC 320 kbps)".to_string(),
                date_generated: "2026-10-12 14:16:00".to_string(),
                size_bytes: 4_800_000,
                verified: true,
            },
        );

        cat.graph.add_derivative(
            "A001_C001",
            DerivativeRecord {
                derivative_id: "A001_C001_thumb".to_string(),
                path: PathBuf::from("/Users/zachshallbetter/Projects/Media/A001_C001_thumb.jpg"),
                operation: OperationKind::Extract,
                preset_name: "Thumbnail (JPEG 1080p)".to_string(),
                date_generated: "2026-10-12 14:17:00".to_string(),
                size_bytes: 420_000,
                verified: true,
            },
        );

        cat
    }

    pub fn add_asset(&mut self, asset: MediaAsset) {
        self.assets.push(asset);
    }

    pub fn all_assets(&self) -> &[MediaAsset] {
        &self.assets
    }

    pub fn assets_mut(&mut self) -> &mut Vec<MediaAsset> {
        &mut self.assets
    }

    pub fn graph(&self) -> &DerivativeGraph {
        &self.graph
    }

    pub fn graph_mut(&mut self) -> &mut DerivativeGraph {
        &mut self.graph
    }

    pub fn get_by_id(&self, id: &str) -> Option<&MediaAsset> {
        self.assets.iter().find(|a| a.id == id)
    }

    /// Query and filter assets.
    pub fn query(
        &self,
        collection: SmartCollection,
        search_query: &str,
        kind_filter: Option<MediaKind>,
    ) -> Vec<&MediaAsset> {
        let q = search_query.trim().to_ascii_lowercase();

        self.assets
            .iter()
            .filter(|a| collection.matches(a))
            .filter(|a| {
                if let Some(kind) = kind_filter {
                    a.kind == kind
                } else {
                    true
                }
            })
            .filter(|a| {
                if q.is_empty() {
                    return true;
                }
                a.filename.to_ascii_lowercase().contains(&q)
                    || a.id.to_ascii_lowercase().contains(&q)
                    || a.camera_model
                        .as_deref()
                        .map(|c| c.to_ascii_lowercase().contains(&q))
                        .unwrap_or(false)
                    || a.video
                        .as_ref()
                        .map(|v| v.codec.to_ascii_lowercase().contains(&q))
                        .unwrap_or(false)
                    || a.tags.iter().any(|t| t.to_ascii_lowercase().contains(&q))
            })
            .collect()
    }

    /// Counts by MediaKind.
    pub fn kind_counts(&self) -> HashMap<MediaKind, usize> {
        let mut counts = HashMap::new();
        for a in &self.assets {
            *counts.entry(a.kind).or_insert(0) += 1;
        }
        counts
    }

    /// Detects exact and near-duplicate assets across the indexed catalog.
    pub fn find_duplicate_clusters(&self) -> Vec<duplicates::DuplicateCluster> {
        let finder = duplicates::DuplicateFinder::new();
        let mut clusters = finder.find_exact_duplicates(&self.assets);
        clusters.extend(finder.find_near_duplicates(&self.assets));
        clusters
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_corpus_loading() {
        let cat = MediaCatalog::with_standard_mock_corpus();
        assert_eq!(cat.all_assets().len(), 15);
        let counts = cat.kind_counts();
        assert_eq!(*counts.get(&MediaKind::Video).unwrap_or(&0), 11);
        assert_eq!(*counts.get(&MediaKind::Audio).unwrap_or(&0), 1);
        assert_eq!(*counts.get(&MediaKind::RawDng).unwrap_or(&0), 1);
    }

    #[test]
    fn test_smart_collection_filtering() {
        let cat = MediaCatalog::with_standard_mock_corpus();
        let four_k = cat.query(SmartCollection::FourKOriginals, "", None);
        assert!(!four_k.is_empty());
        for asset in four_k {
            assert!(
                asset
                    .video
                    .as_ref()
                    .is_some_and(|v| v.width >= 3840 || v.height >= 2160)
            );
        }
    }

    #[test]
    fn test_derivative_graph() {
        let cat = MediaCatalog::with_standard_mock_corpus();
        let derivs = cat.graph().get_derivatives("A001_C001");
        assert_eq!(derivs.len(), 4);
        assert!(derivs.iter().any(|d| d.operation == OperationKind::Proxy));
        assert!(derivs.iter().any(|d| d.operation == OperationKind::Convert));
        assert!(derivs.iter().any(|d| d.operation == OperationKind::Extract));
    }
}
