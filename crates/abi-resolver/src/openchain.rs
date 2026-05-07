//! openchain.xyz / 4byte-style signature lookup, indexed by `selector` only.
//!
//! Selector collisions exist (same 4 bytes can match multiple text signatures
//! across different functions). Each candidate carries a `verified` flag —
//! when openchain has seen the signature on a verified contract — so callers
//! can prefer those over spam-looking ones.
//!
//! Used as a fallback when Sourcify (which is `(chain, address)`-keyed) doesn't
//! have a contract registered.

use serde::Deserialize;
use std::collections::HashMap;

/// One candidate signature for a selector.
#[derive(Debug, Clone, Deserialize)]
pub struct SignatureCandidate {
    /// Canonical Solidity signature (e.g. `approve(address,uint256)`).
    pub signature: String,
    /// True when openchain marked this signature as observed on a verified
    /// contract — a strong proxy for "real function" vs. selector-spam.
    #[serde(default)]
    pub verified: bool,
}

/// In-memory selector → candidates index.
#[derive(Debug, Default)]
pub struct OpenchainIndex {
    by_selector: HashMap<[u8; 4], Vec<SignatureCandidate>>,
}

impl OpenchainIndex {
    /// Empty index — useful before the dump is imported.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Insert one candidate. Multiple candidates for the same selector are
    /// kept and ordered with verified entries first; ties keep insertion order.
    pub fn insert(&mut self, selector: [u8; 4], candidate: SignatureCandidate) {
        let bucket = self.by_selector.entry(selector).or_default();
        bucket.push(candidate);
        bucket.sort_by_key(|c| !c.verified);
    }

    /// Look up every candidate for a selector, verified entries first.
    #[must_use]
    pub fn lookup(&self, selector: [u8; 4]) -> &[SignatureCandidate] {
        self.by_selector
            .get(&selector)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Best-effort single pick: the first verified candidate, else the first
    /// candidate of any kind, else `None`.
    #[must_use]
    pub fn best(&self, selector: [u8; 4]) -> Option<&SignatureCandidate> {
        self.lookup(selector).first()
    }

    /// Number of selectors currently indexed.
    #[must_use]
    pub fn selector_count(&self) -> usize {
        self.by_selector.len()
    }
}

/// On-disk format. Decoupled from openchain's API shape so the import script
/// can normalise once and the runtime never has to think about it.
#[derive(Debug, Deserialize)]
pub struct OpenchainBundle {
    pub entries: Vec<OpenchainEntry>,
}

#[derive(Debug, Deserialize)]
pub struct OpenchainEntry {
    /// Hex selector with the `0x` prefix, e.g. `"0x095ea7b3"`.
    pub selector: String,
    pub signature: String,
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("invalid selector hex: {0}")]
    BadSelector(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl OpenchainIndex {
    /// Load + index a bundle from JSON bytes.
    ///
    /// # Errors
    /// Returns `LoadError::Json` for malformed JSON, or
    /// `LoadError::BadSelector` for any entry whose `selector` isn't a 4-byte
    /// hex string.
    pub fn load_bundle(bytes: &[u8]) -> Result<Self, LoadError> {
        let bundle: OpenchainBundle = serde_json::from_slice(bytes)?;
        let mut index = Self::empty();
        for entry in bundle.entries {
            let selector = parse_selector(&entry.selector)?;
            index.insert(
                selector,
                SignatureCandidate {
                    signature: entry.signature,
                    verified: entry.verified,
                },
            );
        }
        Ok(index)
    }
}

fn parse_selector(hex_str: &str) -> Result<[u8; 4], LoadError> {
    let stripped = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    let bytes = hex::decode(stripped).map_err(|_| LoadError::BadSelector(hex_str.to_string()))?;
    if bytes.len() != 4 {
        return Err(LoadError::BadSelector(hex_str.to_string()));
    }
    Ok([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_lookup_single_candidate() {
        let mut index = OpenchainIndex::empty();
        index.insert(
            [0x09, 0x5e, 0xa7, 0xb3],
            SignatureCandidate {
                signature: "approve(address,uint256)".into(),
                verified: true,
            },
        );
        let candidates = index.lookup([0x09, 0x5e, 0xa7, 0xb3]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].signature, "approve(address,uint256)");
        assert!(candidates[0].verified);
    }

    #[test]
    fn verified_candidates_sort_first() {
        let mut index = OpenchainIndex::empty();
        index.insert(
            [0x09, 0x5e, 0xa7, 0xb3],
            SignatureCandidate {
                signature: "spam_signature(int8[])".into(),
                verified: false,
            },
        );
        index.insert(
            [0x09, 0x5e, 0xa7, 0xb3],
            SignatureCandidate {
                signature: "approve(address,uint256)".into(),
                verified: true,
            },
        );
        let best = index.best([0x09, 0x5e, 0xa7, 0xb3]).unwrap();
        assert_eq!(best.signature, "approve(address,uint256)");
    }

    #[test]
    fn lookup_unknown_selector_returns_empty() {
        let index = OpenchainIndex::empty();
        assert!(index.lookup([0xde, 0xad, 0xbe, 0xef]).is_empty());
        assert!(index.best([0xde, 0xad, 0xbe, 0xef]).is_none());
    }

    #[test]
    fn load_bundle_round_trip() {
        let bundle = serde_json::json!({
            "entries": [
                { "selector": "0x095ea7b3", "signature": "approve(address,uint256)", "verified": true },
                { "selector": "0x095ea7b3", "signature": "spam(int8[])", "verified": false },
                { "selector": "0xa9059cbb", "signature": "transfer(address,uint256)", "verified": true }
            ]
        });
        let bytes = serde_json::to_vec(&bundle).unwrap();
        let index = OpenchainIndex::load_bundle(&bytes).expect("bundle should load");
        assert_eq!(index.selector_count(), 2);
        assert_eq!(index.lookup([0x09, 0x5e, 0xa7, 0xb3]).len(), 2);
        assert_eq!(
            index.best([0x09, 0x5e, 0xa7, 0xb3]).unwrap().signature,
            "approve(address,uint256)"
        );
    }

    #[test]
    fn malformed_selector_rejected() {
        let bundle = serde_json::json!({
            "entries": [
                { "selector": "0xZZ", "signature": "x()", "verified": false }
            ]
        });
        let bytes = serde_json::to_vec(&bundle).unwrap();
        assert!(matches!(
            OpenchainIndex::load_bundle(&bytes).unwrap_err(),
            LoadError::BadSelector(_)
        ));
    }
}
