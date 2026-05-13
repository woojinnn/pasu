//! `RootRequest` — top-level container mirroring `schema_demo/schema/root.json`.

use serde::{Deserialize, Serialize};

use super::common::{Address, DecimalString};
use super::envelope::ActionEnvelope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    Transaction,
    Signature,
    UserOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolRef {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootRequest {
    pub schema_version: String,
    pub request_kind: RequestKind,
    pub chain_id: u64,
    pub from: Address,
    pub to: Address,
    pub value: DecimalString,
    pub selector: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<ProtocolRef>,
    pub actions: Vec<ActionEnvelope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_timestamp: Option<u64>,
}

impl RootRequest {
    pub const SCHEMA_VERSION: &'static str = "1.0.1";
}
