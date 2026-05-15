//! Tier-based resolver. Wraps the in-memory `SourcifyIndex`, the SQLite-backed
//! Sourcify dump (`SqliteSourcifyIndex`), and the openchain selector index,
//! and tries each in priority order for `(chain_id, address, calldata)`:
//!
//! 1. **In-memory Sourcify** (curated bundle, parameter names, EIP-1967-aware)
//! 2. **SQLite Sourcify dump** (~hundreds of thousands of mainnet contracts)
//! 3. **openchain** (selector → signature, no parameter names)
//! 4. `NotFound` (caller decides — typically maps to `LegacyAction::Other` upstream)
//!
//! Decoding is delegated to `crate::decode` once a signature is found.

use crate::decode::{decode_with_function, decode_with_signature, DecodeError, DecodedCall};
use crate::decoder::{CallMatchKey, DecodeContext, DecoderRegistry};
use crate::openchain::OpenchainIndex;
use crate::sourcify::SourcifyIndex;
#[cfg(feature = "sqlite")]
use crate::sqlite_index::SqliteSourcifyIndex;
use alloy_primitives::Address;
use std::sync::Arc;

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
    decoders: Option<Arc<dyn DecoderRegistry>>,
}

impl Resolver {
    #[must_use]
    pub fn new(sourcify: SourcifyIndex, openchain: OpenchainIndex) -> Self {
        Self {
            sourcify,
            #[cfg(feature = "sqlite")]
            sqlite: None,
            openchain,
            decoders: None,
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

    /// Attach a decoder registry fast-path. The decoded result is observed for
    /// now, then resolution continues through the existing Sourcify/openchain
    /// tiers so `ResolveOutcome` remains unchanged.
    #[must_use]
    pub fn with_decoder_registry(mut self, decoders: Arc<dyn DecoderRegistry>) -> Self {
        self.decoders = Some(decoders);
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
            decoders: None,
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
            decoders: None,
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
            decoders: None,
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

        self.try_decoder_fast_path(chain_id, address, selector, calldata);

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

    fn try_decoder_fast_path(
        &self,
        chain_id: u64,
        address: &Address,
        selector: [u8; 4],
        calldata: &[u8],
    ) {
        let Some(decoders) = &self.decoders else {
            return;
        };

        let to = policy_address_from_alloy(address);
        let key = CallMatchKey {
            chain_id,
            to,
            selector,
        };
        let Some(decoder) = decoders.resolve(&key) else {
            return;
        };

        let value: policy_engine::action::DecimalString =
            "0".parse().expect("literal zero is a valid DecimalString");
        let ctx = DecodeContext {
            chain_id,
            to: &key.to,
            value: &value,
            block_timestamp: None,
        };

        match decoder.decode(&ctx, calldata) {
            Ok(decoded_call) => {
                tracing::debug!(
                    decoder_id = decoded_call.decoder_id.as_str(),
                    function_signature = decoded_call.function_signature.as_str(),
                    "decoder matched"
                );
            }
            Err(err) => {
                let decoder_id = decoder.id();
                tracing::debug!(
                    decoder_id = decoder_id.as_str(),
                    error = %err,
                    "decoder matched but decode failed"
                );
            }
        }
    }
}

fn selector_from_calldata(calldata: &[u8]) -> Option<[u8; 4]> {
    if calldata.len() < 4 {
        None
    } else {
        Some([calldata[0], calldata[1], calldata[2], calldata[3]])
    }
}

fn policy_address_from_alloy(address: &Address) -> policy_engine::action::Address {
    format!("0x{}", hex::encode(address.as_slice()))
        .parse()
        .expect("alloy address renders as a valid policy address")
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
    use crate::decoder::{
        CallMatchKey, DecodeContext, DecodedCall as RegistryDecodedCall, Decoder, DecoderError,
        DecoderId,
    };
    use crate::in_memory_registry::InMemoryDecoderRegistry;
    use crate::openchain::SignatureCandidate;
    use alloy_json_abi::Function;
    use alloy_primitives::U256;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct RecordingDecoder {
        key: CallMatchKey,
        calls: Arc<AtomicUsize>,
    }

    impl Decoder for RecordingDecoder {
        fn id(&self) -> DecoderId {
            DecoderId::new("recording")
        }

        fn match_keys(&self) -> Vec<CallMatchKey> {
            vec![self.key.clone()]
        }

        fn decode(
            &self,
            _ctx: &DecodeContext<'_>,
            _calldata: &[u8],
        ) -> Result<RegistryDecodedCall, DecoderError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(RegistryDecodedCall {
                decoder_id: self.id(),
                function_signature: "approve(address,uint256)".into(),
                args: Vec::new(),
                nested: Vec::new(),
            })
        }
    }

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

    #[test]
    fn decoder_registry_match_preserves_existing_outcome_shape() {
        let address = Address::from([0x42u8; 20]);
        let key = CallMatchKey {
            chain_id: 1,
            to: "0x4242424242424242424242424242424242424242"
                .parse()
                .unwrap(),
            selector: [0x09, 0x5e, 0xa7, 0xb3],
        };
        let calls = Arc::new(AtomicUsize::new(0));
        let decoder = Arc::new(RecordingDecoder {
            key,
            calls: Arc::clone(&calls),
        });
        let registry = Arc::new(InMemoryDecoderRegistry::builder().register(decoder).build());
        let resolver = Resolver::empty().with_decoder_registry(registry);

        let calldata = approve_calldata([0x11; 20], 100);
        assert!(matches!(
            resolver.resolve(1, &address, &calldata),
            ResolveOutcome::NotFound
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
