//! Action envelope and action category types.

use serde::{Deserialize, Serialize};

/// High-level category assigned to an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Decentralized exchange activity.
    Dex,
    /// Lending market activity.
    Lending,
    /// Real-world asset activity.
    Rwa,
    /// Liquid staking activity.
    LiquidStaking,
    /// Restaking activity.
    Restaking,
    /// Yield strategy activity.
    Yield,
    /// Miscellaneous activity.
    Misc,
    /// Unknown category.
    Unknown,
}

/// Placeholder action enum, replaced with the full 32 variants in Phase 1.7.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", content = "fields", rename_all = "snake_case")]
pub enum Action {
    /// Placeholder variant until the full action set lands.
    Stub,
}

/// Categorized action envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionEnvelope {
    /// High-level category.
    pub category: Category,
    /// Tagged action payload.
    #[serde(flatten)]
    pub action: Action,
}

#[cfg(test)]
mod tests {
    use super::{Action, ActionEnvelope, Category};

    #[test]
    fn test_category_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&Category::LiquidStaking).unwrap(),
            r#""liquid_staking""#
        );
        assert_eq!(
            serde_json::from_str::<Category>(r#""restaking""#).unwrap(),
            Category::Restaking
        );
    }

    #[test]
    fn test_action_envelope_json_shape() {
        let envelope = ActionEnvelope {
            category: Category::Dex,
            action: Action::Stub,
        };

        let json = serde_json::to_string(&envelope).unwrap();

        assert_eq!(json, r#"{"category":"dex","action":"stub"}"#);
        assert_eq!(
            serde_json::from_str::<ActionEnvelope>(&json).unwrap(),
            envelope
        );
    }
}
