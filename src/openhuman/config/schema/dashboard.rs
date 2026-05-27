//! Dashboard configuration (event stream, future panels).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DashboardConfig {
    #[serde(default)]
    pub event_stream: EventStreamConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct EventStreamConfig {
    /// Whether the live event stream endpoint is enabled.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Maximum number of entries the frontend should retain.
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,

    /// Where new entries appear: "top" (newest first) or "bottom" (oldest first).
    #[serde(default = "default_new_entries")]
    pub new_entries: String,
}

fn default_enabled() -> bool {
    true
}
fn default_max_entries() -> usize {
    200
}
fn default_new_entries() -> String {
    "top".to_string()
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            event_stream: EventStreamConfig::default(),
        }
    }
}

impl Default for EventStreamConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 200,
            new_entries: "top".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_issue_spec() {
        let cfg = DashboardConfig::default();
        assert!(cfg.event_stream.enabled);
        assert_eq!(cfg.event_stream.max_entries, 200);
        assert_eq!(cfg.event_stream.new_entries, "top");
    }

    #[test]
    fn deserialize_from_empty_json() {
        let cfg: DashboardConfig = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(cfg.event_stream.enabled);
        assert_eq!(cfg.event_stream.max_entries, 200);
    }

    #[test]
    fn deserialize_custom_values() {
        let cfg: DashboardConfig = serde_json::from_value(serde_json::json!({
            "event_stream": {
                "enabled": false,
                "max_entries": 500,
                "new_entries": "bottom"
            }
        }))
        .unwrap();
        assert!(!cfg.event_stream.enabled);
        assert_eq!(cfg.event_stream.max_entries, 500);
        assert_eq!(cfg.event_stream.new_entries, "bottom");
    }
}
