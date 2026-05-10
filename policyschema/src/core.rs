//! `NormalizedRequestV2` — 정규화된 최상위 출력.

use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::call::DecodedCall;
use crate::confidence::ConfidenceReport;
use crate::extension::Extension;
use crate::raw::Raw;
use crate::request::Request;
use crate::target::ContractTarget;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedRequestV2 {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub request: Request,
    pub targets: Vec<ContractTarget>,
    #[serde(rename = "decodedCalls")]
    pub decoded_calls: Vec<DecodedCall>,
    pub actions: Vec<Action>,
    pub extensions: Vec<Extension>,
    pub confidence: ConfidenceReport,
    pub raw: Raw,
}
