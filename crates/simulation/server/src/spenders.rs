//! Spender reputation catalog.
//!
//! Loaded from `scopeball-spenders.toml` at server startup (path overridable
//! via `SCOPEBALL_SPENDERS_CONFIG`). Hot-reload is intentionally out of
//! scope — restart the server to pick up edits, keeps the read path zero-lock.
//!
//! The catalog is consulted from:
//! - `GET /spenders/:addr` — direct UI lookups (Monitoring L2 approval list)
//! - approval risk classifier (`GET /wallets/:addr/approvals?with_risk=true`,
//!   Phase 3) — `risk: ["EOA_SPENDER"]` flips to `["KNOWN_VENUE"]` for entries
//!   tagged `rep = "known"`.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// In-memory catalog shared as a clone-cheap snapshot. Keyed by
/// lower-case `0x40hex` address.
#[derive(Clone, Debug, Default)]
pub struct SpenderCatalog {
    by_addr: HashMap<String, SpenderMeta>,
}

impl SpenderCatalog {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Parse a toml document. Returns the parsed entries + a per-entry
    /// error log for rows that failed validation (so a single bad row
    /// doesn't blackout the whole catalog).
    pub fn from_toml(input: &str) -> Result<(Self, Vec<String>), toml::de::Error> {
        let file: SpendersFile = toml::from_str(input)?;
        let mut by_addr = HashMap::with_capacity(file.spenders.len());
        let mut warnings = Vec::new();
        for entry in file.spenders {
            let key = entry.addr.trim().to_lowercase();
            if !is_valid_addr(&key) {
                warnings.push(format!(
                    "skip {key:?}: addr must be lower-case 0x followed by 40 hex chars"
                ));
                continue;
            }
            let meta = SpenderMeta {
                addr: key.clone(),
                label: entry.label,
                rep: entry.rep,
                chain: entry.chain,
                notes: entry.notes,
            };
            by_addr.insert(key, meta);
        }
        Ok((Self { by_addr }, warnings))
    }

    pub fn from_path(path: &Path) -> Result<(Self, Vec<String>), CatalogError> {
        let raw = std::fs::read_to_string(path).map_err(CatalogError::Io)?;
        Self::from_toml(&raw).map_err(CatalogError::Toml)
    }

    #[must_use]
    pub fn get(&self, addr_lower: &str) -> Option<&SpenderMeta> {
        self.by_addr.get(addr_lower)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_addr.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_addr.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("read catalog: {0}")]
    Io(#[source] std::io::Error),
    #[error("parse catalog: {0}")]
    Toml(#[source] toml::de::Error),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpenderMeta {
    pub addr: String,
    pub label: String,
    /// `"known"` or `"blocked"`. Anything missing the address altogether
    /// is implicitly `"unknown"` and the endpoint returns 404.
    pub rep: SpenderRep,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SpenderRep {
    Known,
    Blocked,
}

#[derive(Deserialize)]
struct SpendersFile {
    #[serde(default)]
    spenders: Vec<SpenderEntry>,
}

#[derive(Deserialize)]
struct SpenderEntry {
    addr: String,
    label: String,
    rep: SpenderRep,
    #[serde(default)]
    chain: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

fn is_valid_addr(s: &str) -> bool {
    s.len() == 42 && s.starts_with("0x") && s[2..].chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[[spenders]]
addr = "0xe592427a0aece92de3edee1f18e0157c05861564"
label = "Uniswap V3 SwapRouter"
rep = "known"

[[spenders]]
addr = "0x000000000000000000000000000000000000dead"
label = "burn (example)"
rep = "blocked"
notes = "test entry"
"#;

    #[test]
    fn parses_known_and_blocked() {
        let (cat, warnings) = SpenderCatalog::from_toml(SAMPLE).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(cat.len(), 2);
        let uni = cat
            .get("0xe592427a0aece92de3edee1f18e0157c05861564")
            .unwrap();
        assert_eq!(uni.rep, SpenderRep::Known);
        assert_eq!(uni.label, "Uniswap V3 SwapRouter");
    }

    #[test]
    fn case_normalised_lookup() {
        let toml_input = r#"
[[spenders]]
addr = "0xE592427A0AECE92DE3EDEE1F18E0157C05861564"
label = "Uniswap"
rep = "known"
"#;
        let (cat, warnings) = SpenderCatalog::from_toml(toml_input).unwrap();
        assert!(warnings.is_empty());
        assert!(cat
            .get("0xe592427a0aece92de3edee1f18e0157c05861564")
            .is_some());
        // Upper-case lookup misses — caller must pre-normalize.
        assert!(cat
            .get("0xE592427A0AECE92DE3EDEE1F18E0157C05861564")
            .is_none());
    }

    #[test]
    fn invalid_addr_skipped_with_warning() {
        let bad = r#"
[[spenders]]
addr = "0xnot-hex"
label = "broken"
rep = "known"

[[spenders]]
addr = "0xe592427a0aece92de3edee1f18e0157c05861564"
label = "ok"
rep = "known"
"#;
        let (cat, warnings) = SpenderCatalog::from_toml(bad).unwrap();
        assert_eq!(cat.len(), 1);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("40 hex"));
    }
}
