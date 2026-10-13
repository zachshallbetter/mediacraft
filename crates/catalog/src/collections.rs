//! Dynamic Smart Collections & User Collections Engine for MediaCraft.
//!
//! Provides non-destructive, reference-based asset collections and dynamic rule-based
//! predicates across folders without modifying files or duplicating disk bytes.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::{MediaAsset, MediaKind};

/// Logical operator combining multiple matching rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LogicalOp {
    /// All predicates must match (AND).
    All,
    /// At least one predicate must match (OR).
    Any,
}

/// Dynamic predicate rule for filtering media assets in smart collections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MatchRule {
    /// Asset star rating is at least the given value (1..=5).
    RatingAtLeast(u8),
    /// Asset color label matches the given string (case-insensitive).
    LabelMatches(String),
    /// Asset media kind equals the given kind.
    KindEquals(MediaKind),
    /// Asset video codec contains the given substring (e.g. "hevc", "prores", "avc").
    CodecContains(String),
    /// Video width and height are at least the specified dimensions.
    ResolutionAtLeast { width: u32, height: u32 },
    /// Duration is within [min_sec, max_sec] range inclusive.
    DurationBetweenSec { min_sec: f32, max_sec: f32 },
    /// Asset contains the specified tag / keyword.
    HasTag(String),
    /// Filename contains the given search substring (case-insensitive).
    FilenameContains(String),
    /// Asset is flagged as needing transcode / processing.
    NeedsTranscode(bool),
    /// Asset has a linked editing proxy.
    HasProxy(bool),
    /// Asset is currently disconnected / offline.
    IsOffline(bool),
}

impl MatchRule {
    /// Evaluates this single predicate against a target media asset.
    pub fn matches(&self, asset: &MediaAsset) -> bool {
        match self {
            Self::RatingAtLeast(min_rating) => {
                // In MediaCraft, tags can store rating e.g. "rating:5" or we check tags / sidecar
                let tag_rating = asset.tags.iter().find_map(|t| {
                    if let Some(num_str) = t.strip_prefix("rating:") {
                        num_str.parse::<u8>().ok()
                    } else {
                        None
                    }
                });
                tag_rating.map(|r| r >= *min_rating).unwrap_or(false)
            }
            Self::LabelMatches(label) => asset.tags.iter().any(|t| {
                if let Some(l) = t.strip_prefix("label:") {
                    l.eq_ignore_ascii_case(label)
                } else {
                    t.eq_ignore_ascii_case(label)
                }
            }),
            Self::KindEquals(kind) => asset.kind == *kind,
            Self::CodecContains(codec_sub) => {
                let sub = codec_sub.to_lowercase();
                let in_video = asset
                    .video
                    .as_ref()
                    .map(|v| v.codec.to_lowercase().contains(&sub))
                    .unwrap_or(false);
                let in_audio = asset
                    .audio
                    .as_ref()
                    .map(|a| a.codec.to_lowercase().contains(&sub))
                    .unwrap_or(false);
                in_video || in_audio
            }
            Self::ResolutionAtLeast { width, height } => asset
                .video
                .as_ref()
                .map(|v| v.width >= *width && v.height >= *height)
                .unwrap_or(false),
            Self::DurationBetweenSec { min_sec, max_sec } => {
                let dur = asset.duration_secs.unwrap_or(0.0);
                dur >= *min_sec && dur <= *max_sec
            }
            Self::HasTag(tag) => asset.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)),
            Self::FilenameContains(sub) => {
                asset.filename.to_lowercase().contains(&sub.to_lowercase())
            }
            Self::NeedsTranscode(req) => asset.needs_transcode == *req,
            Self::HasProxy(req) => asset.has_proxy == *req,
            Self::IsOffline(req) => asset.is_offline == *req,
        }
    }
}

/// A group of rules combined by a logical operator (AND / OR).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleGroup {
    pub op: LogicalOp,
    pub rules: Vec<MatchRule>,
}

impl RuleGroup {
    pub fn new(op: LogicalOp, rules: Vec<MatchRule>) -> Self {
        Self { op, rules }
    }

    pub fn matches(&self, asset: &MediaAsset) -> bool {
        if self.rules.is_empty() {
            return true;
        }

        match self.op {
            LogicalOp::All => self.rules.iter().all(|r| r.matches(asset)),
            LogicalOp::Any => self.rules.iter().any(|r| r.matches(asset)),
        }
    }
}

/// A dynamic, rule-based smart collection evaluated on the fly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DynamicSmartCollection {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub rule_group: RuleGroup,
}

impl DynamicSmartCollection {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        icon: impl Into<String>,
        rule_group: RuleGroup,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            icon: icon.into(),
            rule_group,
        }
    }

    pub fn matches(&self, asset: &MediaAsset) -> bool {
        self.rule_group.matches(asset)
    }
}

/// A user-curated static collection holding references to asset IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticCollection {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,
    pub asset_ids: Vec<String>,
}

impl StaticCollection {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        icon: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            icon: icon.into(),
            asset_ids: Vec::new(),
        }
    }

    pub fn contains(&self, asset_id: &str) -> bool {
        self.asset_ids.iter().any(|id| id == asset_id)
    }

    pub fn add_asset(&mut self, asset_id: String) {
        if !self.contains(&asset_id) {
            self.asset_ids.push(asset_id);
        }
    }

    pub fn remove_asset(&mut self, asset_id: &str) {
        self.asset_ids.retain(|id| id != asset_id);
    }
}

/// Manager organizing both static collections and dynamic smart collections.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectionManager {
    pub static_collections: Vec<StaticCollection>,
    pub smart_collections: Vec<DynamicSmartCollection>,
}

impl Default for CollectionManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl CollectionManager {
    /// Creates a manager initialized with professional default smart collections.
    pub fn with_defaults() -> Self {
        let smart_collections = vec![
            DynamicSmartCollection::new(
                "smart_deliverables",
                "Client Deliverables",
                "Assets tagged as finished deliverables ready for client review",
                "🚀",
                RuleGroup::new(
                    LogicalOp::All,
                    vec![MatchRule::HasTag("deliverable".to_string())],
                ),
            ),
            DynamicSmartCollection::new(
                "smart_proxies",
                "Editing Proxies",
                "Assets with generated editing proxies or proxy flag",
                "❏",
                RuleGroup::new(
                    LogicalOp::Any,
                    vec![
                        MatchRule::HasProxy(true),
                        MatchRule::HasTag("proxy".to_string()),
                    ],
                ),
            ),
            DynamicSmartCollection::new(
                "smart_needs_transcode",
                "Needs Transcode",
                "High-bitrate or delivery-unfriendly formats queued for conversion",
                "⚡",
                RuleGroup::new(LogicalOp::All, vec![MatchRule::NeedsTranscode(true)]),
            ),
            DynamicSmartCollection::new(
                "smart_4k_uhd",
                "4K UHD Originals",
                "Master video files with resolution 3840x2160 or higher",
                "🎞",
                RuleGroup::new(
                    LogicalOp::All,
                    vec![
                        MatchRule::KindEquals(MediaKind::Video),
                        MatchRule::ResolutionAtLeast {
                            width: 3840,
                            height: 2160,
                        },
                    ],
                ),
            ),
            DynamicSmartCollection::new(
                "smart_offline",
                "Offline Media",
                "Missing or disconnected assets requiring volume relink",
                "⚠",
                RuleGroup::new(LogicalOp::All, vec![MatchRule::IsOffline(true)]),
            ),
            DynamicSmartCollection::new(
                "smart_prores",
                "ProRes Masters",
                "Master intermediate files encoded in Apple ProRes",
                "💎",
                RuleGroup::new(
                    LogicalOp::All,
                    vec![MatchRule::CodecContains("prores".to_string())],
                ),
            ),
        ];

        let mut b_roll = StaticCollection::new(
            "col_b_roll",
            "Select B-Roll",
            "Curated select takes for the summer documentary sequence",
            "🎬",
        );
        b_roll.add_asset("A001_C001".to_string());
        b_roll.add_asset("B002_C014".to_string());

        let mut hero = StaticCollection::new(
            "col_hero_takes",
            "Hero Takes",
            "Principal aerial landscape establishing shots",
            "⭐",
        );
        hero.add_asset("Drone_003".to_string());

        Self {
            static_collections: vec![b_roll, hero],
            smart_collections,
        }
    }

    /// Filters assets matching a specific smart collection ID.
    pub fn filter_smart<'a>(
        &self,
        smart_id: &str,
        assets: &'a [MediaAsset],
    ) -> Vec<&'a MediaAsset> {
        if let Some(col) = self.smart_collections.iter().find(|c| c.id == smart_id) {
            assets.iter().filter(|a| col.matches(a)).collect()
        } else {
            Vec::new()
        }
    }

    /// Filters assets belonging to a static collection ID.
    pub fn filter_static<'a>(&self, col_id: &str, assets: &'a [MediaAsset]) -> Vec<&'a MediaAsset> {
        if let Some(col) = self.static_collections.iter().find(|c| c.id == col_id) {
            let id_set: HashSet<&str> = col.asset_ids.iter().map(|s| s.as_str()).collect();
            assets
                .iter()
                .filter(|a| id_set.contains(a.id.as_str()))
                .collect()
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MediaCatalog;

    #[test]
    fn test_dynamic_smart_collection_rules() -> Result<(), Box<dyn std::error::Error>> {
        let catalog = MediaCatalog::with_standard_mock_corpus();
        let manager = CollectionManager::with_defaults();

        // Test 4K UHD smart collection
        let four_k = manager.filter_smart("smart_4k_uhd", catalog.all());
        assert!(!four_k.is_empty());
        for a in four_k {
            let v = a.video.as_ref().ok_or("has video")?;
            assert!(v.width >= 3840 && v.height >= 2160);
        }

        // Test Offline smart collection
        let offline = manager.filter_smart("smart_offline", catalog.all());
        assert_eq!(offline.len(), 1);
        assert_eq!(offline[0].filename, "Offline_Take_04.mov");

        // Test ProRes smart collection
        let prores = manager.filter_smart("smart_prores", catalog.all());
        assert!(!prores.is_empty());
        for a in prores {
            let codec = &a.video.as_ref().ok_or("video")?.codec;
            assert!(codec.to_lowercase().contains("prores"));
        }
        Ok(())
    }

    #[test]
    fn test_static_collection_membership() {
        let catalog = MediaCatalog::with_standard_mock_corpus();
        let mut manager = CollectionManager::with_defaults();

        let mut custom = StaticCollection::new("test_picks", "Director Picks", "Notes", "📌");
        custom.add_asset("A001_C001".to_string());
        custom.add_asset("B002_C014".to_string());
        manager.static_collections.push(custom);

        let filtered = manager.filter_static("test_picks", catalog.all());
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|a| a.id == "A001_C001"));
        assert!(filtered.iter().any(|a| a.id == "B002_C014"));
    }
}
