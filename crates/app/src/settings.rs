//! MediaCraft Preferences & State Management using craft-settings

use craft_settings::{Location, Settings, SettingsStore};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct MediaCraftPreferences {
    pub default_output_folder: String,
    pub thumbnail_zoom: f32,
    pub target_bitrate_kbps: u32,
    pub faststart_enabled: bool,
    pub audio_priming_enabled: bool,
    pub auto_pause_on_playback: bool,
}

impl Default for MediaCraftPreferences {
    fn default() -> Self {
        Self {
            default_output_folder: "/Users/zachshallbetter/Downloads".to_string(),
            thumbnail_zoom: 160.0,
            target_bitrate_kbps: 45_000,
            faststart_enabled: true,
            audio_priming_enabled: true,
            auto_pause_on_playback: true,
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
