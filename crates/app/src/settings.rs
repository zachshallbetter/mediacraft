//! MediaCraft Preferences & State Management using craft-settings

use craft_settings::{Location, Settings, SettingsStore};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct MediaCraftPreferences {
    pub default_output_folder: String,
    pub scratch_disk_folder: String,
    pub thumbnail_zoom: f32,
    pub target_bitrate_kbps: u32,
    pub faststart_enabled: bool,
    pub audio_priming_enabled: bool,
    pub auto_pause_on_playback: bool,
    pub hardware_engine: String,
    pub hardware_decode: bool,
    pub worker_threads: usize,
    pub zero_copy_buffers: bool,
    pub thermal_throttling: bool,
    pub default_color_space: String,
    pub tone_mapping_operator: String,
    pub target_loudness_lufs: f32,
}

impl Default for MediaCraftPreferences {
    fn default() -> Self {
        Self {
            default_output_folder: "/Users/zachshallbetter/Downloads".to_string(),
            scratch_disk_folder: "/tmp/mediacraft-scratch".to_string(),
            thumbnail_zoom: 160.0,
            target_bitrate_kbps: 45_000,
            faststart_enabled: true,
            audio_priming_enabled: true,
            auto_pause_on_playback: true,
            hardware_engine: "Apple VideoToolbox (Hardware)".to_string(),
            hardware_decode: true,
            worker_threads: 8,
            zero_copy_buffers: true,
            thermal_throttling: false,
            default_color_space: "Rec.709 (BT.709)".to_string(),
            tone_mapping_operator: "Hable / Filmic Curve".to_string(),
            target_loudness_lufs: -23.0,
        }
    }
}

impl Settings for MediaCraftPreferences {
    const APP_ID: &'static str = "mediacraft";
    const SCHEMA_VERSION: u32 = 1;
}

pub fn open_settings() -> SettingsStore<MediaCraftPreferences> {
    let (store, _report) = SettingsStore::<MediaCraftPreferences>::open(Location::Auto);
    store
}
