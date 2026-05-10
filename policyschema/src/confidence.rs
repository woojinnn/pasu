//! `Confidence` — 4단계 enum + 단계별 report.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Unavailable,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Request,
    TargetIdentification,
    AbiDecode,
    ProtocolDecode,
    SemanticAction,
    RouteDecode,
    AmountInterpretation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfidenceReport {
    pub overall: Confidence,
    #[serde(default)]
    pub stages: HashMap<Stage, Confidence>,
    #[serde(default)]
    pub notes: Vec<String>,
}
