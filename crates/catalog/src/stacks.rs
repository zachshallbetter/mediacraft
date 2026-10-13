//! Asset Stacks & Versioning Engine (FEATURES.md:L40).
//!
//! Provides grouping of related takes, originals, proxies, and revisions
//! into logical asset stacks with active version promotion.

use serde::{Deserialize, Serialize};

/// High-level categorization of a stacked asset version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackVersionKind {
    /// Original capture or camera master.
    Original,
    /// Alternate take number (e.g. Take 1, Take 2).
    Take(u32),
    /// Revision or edit iteration (e.g. v1, v2, v3).
    Revision(u32),
    /// Low-bitrate lightweight editing proxy.
    Proxy,
    /// Transcoded derivative for specific target (e.g. Web, Broadcast, Social).
    Derivative(String),
}

impl StackVersionKind {
    pub fn label(&self) -> String {
        match self {
            Self::Original => "Original Master".to_string(),
            Self::Take(n) => format!("Take {n}"),
            Self::Revision(n) => format!("Rev v{n}"),
            Self::Proxy => "Editing Proxy".to_string(),
            Self::Derivative(target) => format!("Derivative ({target})"),
        }
    }
}

/// A specific version entry contained in an asset stack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StackedVersion {
    pub asset_id: String,
    pub version_kind: StackVersionKind,
    pub date_added: String,
    pub note: Option<String>,
}

/// A logical asset stack grouping related media assets together.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetStack {
    pub id: String,
    pub name: String,
    /// The ID of the asset currently representing the stack in views.
    pub primary_asset_id: String,
    pub versions: Vec<StackedVersion>,
    pub is_collapsed: bool,
}

impl AssetStack {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        primary_asset_id: impl Into<String>,
    ) -> Self {
        let primary_id = primary_asset_id.into();
        Self {
            id: id.into(),
            name: name.into(),
            primary_asset_id: primary_id.clone(),
            versions: vec![StackedVersion {
                asset_id: primary_id,
                version_kind: StackVersionKind::Original,
                date_added: "2026-10-12T10:00:00Z".to_string(),
                note: Some("Initial master".to_string()),
            }],
            is_collapsed: true,
        }
    }

    /// Adds a new version to the stack.
    pub fn add_version(
        &mut self,
        asset_id: impl Into<String>,
        kind: StackVersionKind,
        note: Option<String>,
    ) {
        let id = asset_id.into();
        if !self.versions.iter().any(|v| v.asset_id == id) {
            self.versions.push(StackedVersion {
                asset_id: id,
                version_kind: kind,
                date_added: "2026-10-12T12:00:00Z".to_string(),
                note,
            });
        }
    }

    /// Promotes a specific asset in the stack to become the primary active version.
    pub fn set_primary(&mut self, asset_id: &str) -> bool {
        if self.versions.iter().any(|v| v.asset_id == asset_id) {
            self.primary_asset_id = asset_id.to_string();
            true
        } else {
            false
        }
    }

    /// Toggles the collapsed/expanded UI state of this stack.
    pub fn toggle_collapsed(&mut self) {
        self.is_collapsed = !self.is_collapsed;
    }

    /// Total number of versions inside this stack.
    pub fn count(&self) -> usize {
        self.versions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_stack_lifecycle() {
        let mut stack = AssetStack::new("stack_001", "Summer Campaign Hero", "asset_orig");
        assert_eq!(stack.count(), 1);
        assert_eq!(stack.primary_asset_id, "asset_orig");
        assert!(stack.is_collapsed);

        stack.add_version(
            "asset_proxy",
            StackVersionKind::Proxy,
            Some("1080p ProRes Proxy".to_string()),
        );
        stack.add_version(
            "asset_rev2",
            StackVersionKind::Revision(2),
            Some("Color graded cut".to_string()),
        );

        assert_eq!(stack.count(), 3);

        // Promote revision 2 to primary
        let success = stack.set_primary("asset_rev2");
        assert!(success);
        assert_eq!(stack.primary_asset_id, "asset_rev2");

        // Attempt invalid primary
        let fail = stack.set_primary("non_existent");
        assert!(!fail);
        assert_eq!(stack.primary_asset_id, "asset_rev2");

        // Toggle collapsed
        stack.toggle_collapsed();
        assert!(!stack.is_collapsed);
    }
}
