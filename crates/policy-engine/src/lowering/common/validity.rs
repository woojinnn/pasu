use crate::action::{Validity, ValiditySource};
use serde_json::{Map, Value};

const EXPIRES_AT: &str = "expiresAt";
const SOURCE: &str = "source";

pub(crate) fn validity_json(validity: &Validity) -> Value {
    let mut out = Map::new();
    out.insert(
        EXPIRES_AT.into(),
        Value::from(validity.expires_at.to_string()),
    );
    out.insert(
        SOURCE.into(),
        Value::from(validity_source_str(&validity.source)),
    );
    Value::Object(out)
}

// Phase 2 left this helper in place for prospective use; manifest-driven
// enrichment now derives `validityDeltaSec` from a `clock.validity_delta_sec`
// RPC call instead of from the host's block_timestamp.
#[allow(dead_code)]
pub(crate) fn validity_delta_sec(validity: &Validity, block_timestamp: u64) -> Option<i64> {
    let expires_at = validity.expires_at.to_string().parse::<i64>().ok()?;
    if expires_at < 0 {
        return None;
    }
    let block_timestamp = i64::try_from(block_timestamp).ok()?;
    Some(expires_at - block_timestamp)
}

pub(crate) const fn validity_source_str(source: &ValiditySource) -> &'static str {
    match source {
        ValiditySource::TxDeadline => "tx-deadline",
        ValiditySource::SignatureDeadline => "signature-deadline",
        ValiditySource::GrantExpiration => "grant-expiration",
    }
}
