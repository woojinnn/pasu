//! Tier-based resolver. Wraps the in-memory `SourcifyIndex`, the SQLite-backed
//! Sourcify dump (`SqliteSourcifyIndex`), and the openchain selector index,
//! and tries each in priority order for `(chain_id, address, calldata)`:
//!
//! 1. **In-memory Sourcify** (curated bundle, parameter names, EIP-1967-aware)
//! 2. **SQLite Sourcify dump** (~hundreds of thousands of mainnet contracts)
//! 3. **openchain** (selector → signature, no parameter names)
//! 4. `NotFound` (caller decides — typically maps to `Action::Other` upstream)
//!
//! Decoding is delegated to `crate::decode` once a signature is found.

use crate::decode::{decode_with_function, decode_with_signature, DecodeError, DecodedCall};
use crate::openchain::OpenchainIndex;
use crate::sourcify::SourcifyIndex;
#[cfg(feature = "sqlite")]
use crate::sqlite_index::SqliteSourcifyIndex;
use alloy_primitives::Address;

/// Where the matching signature came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// In-memory Sourcify hit — curated, argument names available.
    Sourcify,
    /// SQLite Sourcify dump hit — broad coverage, argument names available.
    /// Only emitted when the `sqlite` Cargo feature is enabled.
    #[cfg(feature = "sqlite")]
    SourcifyDb,
    /// openchain fallback — argument names are synthesised (`arg0`, `arg1`).
    Openchain,
}

/// Successful resolution.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub source: Source,
    pub decoded: DecodedCall,
}

/// Outcome of resolving a transaction.
#[derive(Debug)]
pub enum ResolveOutcome {
    /// One of the tiers produced a decode.
    Resolved(Resolved),
    /// No tier matched the selector (or the matched signature failed to decode).
    NotFound,
}

/// Resolver bundling all signature backends. Backends are tried in priority
/// order: curated in-memory Sourcify → SQLite Sourcify dump → openchain.
///
/// Each backend is optional. Construct with [`Resolver::new`] (no SQLite),
/// then attach a SQLite dump with [`Resolver::with_sqlite`] when available.
pub struct Resolver {
    sourcify: SourcifyIndex,
    #[cfg(feature = "sqlite")]
    sqlite: Option<SqliteSourcifyIndex>,
    openchain: OpenchainIndex,
}

impl Resolver {
    #[must_use]
    pub fn new(sourcify: SourcifyIndex, openchain: OpenchainIndex) -> Self {
        Self {
            sourcify,
            #[cfg(feature = "sqlite")]
            sqlite: None,
            openchain,
        }
    }

    /// Attach a SQLite-backed Sourcify dump as a secondary tier (after the
    /// in-memory curated bundle, before openchain). Only available when the
    /// `sqlite` Cargo feature is enabled.
    #[cfg(feature = "sqlite")]
    #[must_use]
    pub fn with_sqlite(mut self, db: SqliteSourcifyIndex) -> Self {
        self.sqlite = Some(db);
        self
    }

    /// Sourcify-only resolver (openchain index empty).
    #[must_use]
    pub fn from_sourcify(sourcify: SourcifyIndex) -> Self {
        Self {
            sourcify,
            #[cfg(feature = "sqlite")]
            sqlite: None,
            openchain: OpenchainIndex::empty(),
        }
    }

    /// openchain-only resolver (Sourcify index empty).
    #[must_use]
    pub fn from_openchain(openchain: OpenchainIndex) -> Self {
        Self {
            sourcify: SourcifyIndex::empty(),
            #[cfg(feature = "sqlite")]
            sqlite: None,
            openchain,
        }
    }

    /// Empty resolver — every lookup returns `NotFound`. Useful in tests.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            sourcify: SourcifyIndex::empty(),
            #[cfg(feature = "sqlite")]
            sqlite: None,
            openchain: OpenchainIndex::empty(),
        }
    }

    /// Try every tier in priority order. Returns `Resolved` on first hit, or
    /// `NotFound` if every tier whiffs (or every candidate fails to decode).
    ///
    /// `calldata` must include the 4-byte selector prefix.
    #[must_use]
    pub fn resolve(&self, chain_id: u64, address: &Address, calldata: &[u8]) -> ResolveOutcome {
        let Some(selector) = selector_from_calldata(calldata) else {
            return ResolveOutcome::NotFound;
        };

        // Tier 1 — In-memory Sourcify (curated).
        if let Some(info) = self.sourcify.lookup(chain_id, address, selector) {
            if let Ok(decoded) = decode_with_function(&info.function, calldata) {
                return ResolveOutcome::Resolved(Resolved {
                    source: Source::Sourcify,
                    decoded,
                });
            }
        }

        // Tier 2 — SQLite Sourcify dump (only when the `sqlite` feature is on).
        #[cfg(feature = "sqlite")]
        if let Some(db) = &self.sqlite {
            if let Ok(Some(info)) = db.lookup(chain_id, address, selector) {
                if let Ok(decoded) = decode_with_function(&info.function, calldata) {
                    return ResolveOutcome::Resolved(Resolved {
                        source: Source::SourcifyDb,
                        decoded,
                    });
                }
            }
        }

        // Tier 3 — openchain. Walk every candidate (verified first) and take
        // the first one that decodes cleanly. Selector-collision spam usually
        // fails to decode against real calldata, so this naturally selects
        // the right signature.
        for candidate in self.openchain.lookup(selector) {
            if let Ok(decoded) = decode_with_signature(&candidate.signature, calldata) {
                return ResolveOutcome::Resolved(Resolved {
                    source: Source::Openchain,
                    decoded,
                });
            }
        }

        ResolveOutcome::NotFound
    }
}

fn selector_from_calldata(calldata: &[u8]) -> Option<[u8; 4]> {
    if calldata.len() < 4 {
        None
    } else {
        Some([calldata[0], calldata[1], calldata[2], calldata[3]])
    }
}

/// Re-exported here so `DecodeError` is reachable from the same module the
/// resolver lives in (callers don't need to import `crate::decode` separately
/// for error matching).
pub use crate::decode::DecodeError as ResolveDecodeError;

// Above re-export silences a "unused" warning when the trait surface compiles
// without anything actually constructing a `DecodeError` here.
const _: fn() = || {
    let _ = std::mem::size_of::<DecodeError>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openchain::SignatureCandidate;
    use alloy_json_abi::Function;
    use alloy_primitives::U256;

    fn approve_calldata(spender: [u8; 20], amount: u128) -> Vec<u8> {
        let mut data = vec![0x09, 0x5e, 0xa7, 0xb3];
        let mut spender_word = [0u8; 32];
        spender_word[12..].copy_from_slice(&spender);
        data.extend_from_slice(&spender_word);
        data.extend_from_slice(&U256::from(amount).to_be_bytes::<32>());
        data
    }

    fn approve_function_with_names() -> Function {
        let abi_json = serde_json::json!({
            "name": "approve",
            "type": "function",
            "inputs": [
                { "name": "spender", "type": "address" },
                { "name": "amount",  "type": "uint256" }
            ],
            "outputs": [{ "name": "", "type": "bool" }],
            "stateMutability": "nonpayable"
        });
        serde_json::from_value(abi_json).unwrap()
    }

    #[test]
    fn sourcify_hit_returns_named_args() {
        let mut sourcify = SourcifyIndex::empty();
        let address = Address::from([0x42u8; 20]);
        sourcify.insert_contract(1, address, &[approve_function_with_names()]);
        let resolver = Resolver::from_sourcify(sourcify);

        let calldata = approve_calldata([0x11; 20], 12345);
        match resolver.resolve(1, &address, &calldata) {
            ResolveOutcome::Resolved(Resolved {
                source: Source::Sourcify,
                decoded,
            }) => {
                assert_eq!(decoded.function_name, "approve");
                assert_eq!(decoded.args[0].name, "spender");
                assert_eq!(decoded.args[1].name, "amount");
            }
            other => panic!("expected Sourcify hit, got {other:?}"),
        }
    }

    #[test]
    fn falls_back_to_openchain_when_sourcify_misses() {
        let mut openchain = OpenchainIndex::empty();
        openchain.insert(
            [0x09, 0x5e, 0xa7, 0xb3],
            SignatureCandidate {
                signature: "approve(address,uint256)".into(),
                verified: true,
            },
        );
        let resolver = Resolver::from_openchain(openchain);

        let calldata = approve_calldata([0x11; 20], 12345);
        let address = Address::from([0x42u8; 20]); // unknown to Sourcify
        match resolver.resolve(1, &address, &calldata) {
            ResolveOutcome::Resolved(Resolved {
                source: Source::Openchain,
                decoded,
            }) => {
                assert_eq!(decoded.function_name, "approve");
                assert_eq!(decoded.args[0].name, "arg0"); // openchain has no names
            }
            other => panic!("expected openchain hit, got {other:?}"),
        }
    }

    #[test]
    fn openchain_skips_collision_spam_that_fails_decode() {
        let mut openchain = OpenchainIndex::empty();
        // Spam signature listed first (verified=false sorts after verified
        // ones, but here both are unverified — first inserted ends up first).
        openchain.insert(
            [0x09, 0x5e, 0xa7, 0xb3],
            SignatureCandidate {
                signature: "_SIMONdotBLACK_(int8[],int224[],int256,int64,uint248[])".into(),
                verified: false,
            },
        );
        openchain.insert(
            [0x09, 0x5e, 0xa7, 0xb3],
            SignatureCandidate {
                signature: "approve(address,uint256)".into(),
                verified: false,
            },
        );
        let resolver = Resolver::from_openchain(openchain);

        let calldata = approve_calldata([0x11; 20], 100);
        let address = Address::from([0x42u8; 20]);
        match resolver.resolve(1, &address, &calldata) {
            ResolveOutcome::Resolved(Resolved {
                source: Source::Openchain,
                decoded,
            }) => {
                // Spam signature would fail to decode, so resolver moves on.
                assert_eq!(decoded.function_name, "approve");
            }
            other => panic!("expected openchain to fall through, got {other:?}"),
        }
    }

    #[test]
    fn empty_resolver_returns_not_found() {
        let resolver = Resolver::empty();
        let address = Address::from([0x42u8; 20]);
        let calldata = approve_calldata([0x11; 20], 100);
        assert!(matches!(
            resolver.resolve(1, &address, &calldata),
            ResolveOutcome::NotFound
        ));
    }

    #[test]
    fn short_calldata_returns_not_found() {
        let resolver = Resolver::empty();
        let address = Address::from([0x42u8; 20]);
        assert!(matches!(
            resolver.resolve(1, &address, &[0x09, 0x5e]),
            ResolveOutcome::NotFound
        ));
    }
}
