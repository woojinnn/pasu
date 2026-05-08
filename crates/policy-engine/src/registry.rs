//! Adapter registry abstractions.
//!
//! - `AdapterRegistry` trait — object-safe contract every registry honors;
//!   `Pipeline` is generic over it so hosts can plug in their own
//!   implementations (caching, hot-reload, remote-mirror, …).
//! - `ResolverOutcome` — what `lookup` / `resolve_with_adapter` returns.
//! - `AdapterIndex` — internal `(chain_id, to, selector)` map shared by
//!   `MockAdapterRegistry` and any future in-memory variants.
//! - `MockAdapterRegistry` — the v0.1 in-memory registry used by tests,
//!   examples, and the `adapters-bundle` aggregator.

#[cfg(test)]
use crate::adapter::MatchKey;
use crate::adapter::{Adapter, AdapterFactory, AdapterId, SignatureAdapter};
use crate::core::{Address, ChainId, SignatureRequest, TransactionRequest};
use std::collections::HashMap;
use std::sync::Arc;

type ExactKey = (ChainId, Address, [u8; 4]);
type WildcardTargetKey = (ChainId, [u8; 4]);
type AdapterList = Vec<Arc<dyn Adapter>>;
type SignatureKey = (ChainId, Address, String);

/// Result of resolving a transaction against installed adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolverOutcome {
    /// Exactly one adapter matched the (`chain_id`, to, selector) key.
    Resolved(AdapterId),
    /// No adapter matched. The pipeline should emit `Action::Other`.
    NoMatch,
    /// Two or more adapters matched. v0.1 surfaces the candidate ids; the
    /// pipeline rejects the transaction until the user pins one.
    Ambiguous(Vec<AdapterId>),
}

/// Result of resolving an EIP-712 signature against installed adapters.
pub enum SignatureResolverOutcome<'a> {
    /// Exactly one signature adapter matched the
    /// (`chain_id`, `verifyingContract`, `primaryType`) key.
    Resolved(&'a dyn SignatureAdapter),
    /// No signature adapter matched. The pipeline should emit
    /// `Action::Eip712Other`.
    NoMatch,
    /// Two or more signature adapters matched.
    Ambiguous(Vec<AdapterId>),
}

impl std::fmt::Debug for SignatureResolverOutcome<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Resolved(adapter) => f.debug_tuple("Resolved").field(&adapter.id()).finish(),
            Self::NoMatch => f.write_str("NoMatch"),
            Self::Ambiguous(ids) => f.debug_tuple("Ambiguous").field(ids).finish(),
        }
    }
}

/// Trait implemented by registry types. Implementors expose
/// `resolve_with_adapter` (the primary method); `lookup` has a default impl
/// for callers that only need the lookup outcome.
///
/// The trait is object-safe (`&dyn AdapterRegistry` works), so hosts can swap
/// in remote-cache-backed registries, hot-reload registries, etc., without
/// changing `Pipeline`.
pub trait AdapterRegistry: Send + Sync {
    /// Resolve a transaction to an adapter. When the outcome is `Resolved`,
    /// the second tuple element carries the adapter `Arc` so the caller can
    /// invoke `build` and keep sequencing in the pipeline without a
    /// second lookup. For `NoMatch` / `Ambiguous`, the second element is `None`.
    fn resolve_with_adapter(
        &self,
        tx: &TransactionRequest,
    ) -> (ResolverOutcome, Option<Arc<dyn Adapter>>);

    /// Convenience: outcome only.
    fn lookup(&self, tx: &TransactionRequest) -> ResolverOutcome {
        self.resolve_with_adapter(tx).0
    }
}

/// Registry for off-chain signature adapters.
pub trait SignatureRegistry: Send + Sync {
    /// Resolve a signature request to a specific adapter.
    fn resolve<'a>(&'a self, sig: &SignatureRequest) -> SignatureResolverOutcome<'a>;
}

/// Index over installed adapters, keyed by `(chain_id, to, selector)` plus a
/// wildcard-target bucket for selectors with `to == None` matchers.
#[derive(Default)]
pub struct AdapterIndex {
    exact: HashMap<ExactKey, AdapterList>,
    wildcard_target: HashMap<WildcardTargetKey, AdapterList>,
}

impl std::fmt::Debug for AdapterIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdapterIndex")
            .field("exact_len", &self.exact.len())
            .field("wildcard_target_len", &self.wildcard_target.len())
            .finish()
    }
}

impl AdapterIndex {
    /// Insert all match keys exposed by `adapter`.
    #[allow(clippy::needless_pass_by_value)]
    pub fn insert(&mut self, adapter: Arc<dyn Adapter>) {
        for key in adapter.match_keys() {
            match key.to {
                Some(to) => self
                    .exact
                    .entry((key.chain_id, to, key.selector))
                    .or_default()
                    .push(Arc::clone(&adapter)),
                None => self
                    .wildcard_target
                    .entry((key.chain_id, key.selector))
                    .or_default()
                    .push(Arc::clone(&adapter)),
            }
        }
    }

    /// Return every adapter matching the transaction selector and target.
    #[must_use]
    pub fn matches_for(&self, tx: &TransactionRequest) -> Vec<Arc<dyn Adapter>> {
        let Some(selector) = tx.selector() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let key = (tx.chain_id, tx.to.clone(), selector);
        if let Some(v) = self.exact.get(&key) {
            out.extend(v.iter().cloned());
        }
        if let Some(v) = self.wildcard_target.get(&(tx.chain_id, selector)) {
            out.extend(v.iter().cloned());
        }
        out
    }
}

/// In-memory adapter registry. The full design (spec §5.3) maintains a
/// host-side cache populated from a remote registry; v0.1 just keeps adapters
/// in memory and resolves by `(chain_id, to, selector)`.
#[derive(Debug, Default)]
pub struct MockAdapterRegistry {
    index: AdapterIndex,
}

impl MockAdapterRegistry {
    /// Construct an empty in-memory registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install an adapter. Returns `self` for builder-style chaining.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn with_adapter(mut self, adapter: Arc<dyn Adapter>) -> Self {
        self.index.insert(adapter);
        self
    }

    /// Instantiate and install an adapter from a registry/catalog factory.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn with_factory<F>(self, factory: F) -> Self
    where
        F: AdapterFactory,
    {
        self.with_adapter(factory.create())
    }
}

impl AdapterRegistry for MockAdapterRegistry {
    fn resolve_with_adapter(
        &self,
        tx: &TransactionRequest,
    ) -> (ResolverOutcome, Option<Arc<dyn Adapter>>) {
        let matches = self.index.matches_for(tx);
        match matches.len() {
            0 => (ResolverOutcome::NoMatch, None),
            1 => (
                ResolverOutcome::Resolved(matches[0].id()),
                Some(Arc::clone(&matches[0])),
            ),
            _ => {
                let ids = matches.iter().map(|a| a.id()).collect();
                (ResolverOutcome::Ambiguous(ids), None)
            }
        }
    }

    // `lookup` uses the trait's default impl (calls `resolve_with_adapter`).
}

/// In-memory signature adapter registry keyed by EIP-712 match fields.
#[derive(Default)]
pub struct MockSignatureRegistry {
    adapters: Vec<Arc<dyn SignatureAdapter>>,
    index: HashMap<SignatureKey, Vec<usize>>,
}

impl std::fmt::Debug for MockSignatureRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockSignatureRegistry")
            .field("adapter_len", &self.adapters.len())
            .field("index_len", &self.index.len())
            .finish()
    }
}

impl MockSignatureRegistry {
    /// Construct an empty in-memory signature registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install a signature adapter. Returns `self` for builder-style chaining.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn with_adapter(mut self, adapter: Arc<dyn SignatureAdapter>) -> Self {
        let adapter_index = self.adapters.len();
        for key in adapter.match_keys() {
            self.index
                .entry((
                    key.chain_id,
                    key.verifying_contract,
                    normalize_primary_type(&key.primary_type),
                ))
                .or_default()
                .push(adapter_index);
        }
        self.adapters.push(adapter);
        self
    }
}

impl SignatureRegistry for MockSignatureRegistry {
    fn resolve<'a>(&'a self, sig: &SignatureRequest) -> SignatureResolverOutcome<'a> {
        let key = (
            sig.chain_id,
            sig.typed_data.domain.verifying_contract.clone(),
            normalize_primary_type(&sig.typed_data.primary_type),
        );
        let Some(adapter_indices) = self.index.get(&key) else {
            return SignatureResolverOutcome::NoMatch;
        };

        match adapter_indices.as_slice() {
            [] => SignatureResolverOutcome::NoMatch,
            [adapter_index] => self
                .adapters
                .get(*adapter_index)
                .map_or(SignatureResolverOutcome::NoMatch, |adapter| {
                    SignatureResolverOutcome::Resolved(Arc::as_ref(adapter))
                }),
            _ => {
                let ids = adapter_indices
                    .iter()
                    .filter_map(|adapter_index| self.adapters.get(*adapter_index))
                    .map(|adapter| adapter.id())
                    .collect();
                SignatureResolverOutcome::Ambiguous(ids)
            }
        }
    }
}

fn normalize_primary_type(primary_type: &str) -> String {
    primary_type.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{Adapter, AdapterError, SignatureMatchKey};
    use crate::core::{
        Action, Eip712Domain, Eip712TypedData, OtherAction, SignatureRequest, TransactionRequest,
    };
    use serde_json::json;

    /// Minimal in-crate adapter used only to exercise the registry. Doesn't
    /// touch ABI decoding or oracle data — it just claims a fixed set of
    /// match keys and returns `Action::Other` from `build`.
    struct TestAdapter {
        id: AdapterId,
        keys: Vec<MatchKey>,
    }

    struct TestSignatureAdapter {
        id: AdapterId,
        keys: Vec<SignatureMatchKey>,
    }

    impl Adapter for TestAdapter {
        fn id(&self) -> AdapterId {
            self.id.clone()
        }
        fn match_keys(&self) -> Vec<MatchKey> {
            self.keys.clone()
        }
        fn build(&self, tx: &TransactionRequest) -> Result<Action, AdapterError> {
            Ok(Action::Other(OtherAction {
                actor: tx.from.clone(),
                target: tx.to.clone(),
                selector: tx.selector_hex().unwrap_or_else(|| "0x".into()),
                value_wei: tx.value_wei.clone(),
                raw_calldata: format!("0x{}", hex::encode(&tx.data)),
            }))
        }
    }

    impl SignatureAdapter for TestSignatureAdapter {
        fn id(&self) -> AdapterId {
            self.id.clone()
        }

        fn match_keys(&self) -> Vec<SignatureMatchKey> {
            self.keys.clone()
        }

        fn build(&self, _sig: &SignatureRequest) -> Result<Action, AdapterError> {
            Ok(Action::Other(OtherAction {
                actor: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
                target: fixed_target(),
                selector: "0x".into(),
                value_wei: "0".into(),
                raw_calldata: "0x".into(),
            }))
        }
    }

    fn fixed_target() -> Address {
        Address::new("0x1111111111111111111111111111111111111111").unwrap()
    }

    fn sample_tx() -> TransactionRequest {
        let mut data = vec![0xaa, 0xbb, 0xcc, 0xdd];
        data.extend_from_slice(&[0u8; 32]);
        TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: fixed_target(),
            value_wei: "0".into(),
            data,
            gas: None,
            nonce: None,
        }
    }

    fn test_adapter(id: &str) -> Arc<dyn Adapter> {
        Arc::new(TestAdapter {
            id: AdapterId::new(id).expect("static AdapterId is well-formed"),
            keys: vec![MatchKey::exact(1, fixed_target(), [0xaa, 0xbb, 0xcc, 0xdd])],
        })
    }

    fn sample_sig(primary_type: &str) -> SignatureRequest {
        SignatureRequest {
            chain_id: 1,
            signer: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            typed_data: Eip712TypedData {
                domain: Eip712Domain {
                    name: Some("Example".into()),
                    version: Some("1".into()),
                    chain_id: 1,
                    verifying_contract: fixed_target(),
                    salt: None,
                },
                primary_type: primary_type.into(),
                types: json!({
                    primary_type: [
                        { "name": "value", "type": "uint256" }
                    ]
                }),
                message: json!({ "value": "1" }),
            },
        }
    }

    fn test_signature_adapter(id: &str) -> Arc<dyn SignatureAdapter> {
        Arc::new(TestSignatureAdapter {
            id: AdapterId::new(id).expect("static AdapterId is well-formed"),
            keys: vec![SignatureMatchKey::exact(1, fixed_target(), "Permit")],
        })
    }

    #[test]
    fn registry_resolves_single_match() {
        let reg = MockAdapterRegistry::new().with_adapter(test_adapter("test/a@1"));
        let outcome = reg.lookup(&sample_tx());
        assert_eq!(
            outcome,
            ResolverOutcome::Resolved(
                AdapterId::new("test/a@1").expect("static AdapterId is well-formed")
            )
        );
    }

    #[test]
    fn registry_no_match_when_target_address_differs() {
        let reg = MockAdapterRegistry::new().with_adapter(test_adapter("test/a@1"));
        let mut tx = sample_tx();
        tx.to = Address::new("0x000000000000000000000000000000000000dead").unwrap();
        assert_eq!(reg.lookup(&tx), ResolverOutcome::NoMatch);
    }

    #[test]
    fn registry_no_match_when_selector_differs() {
        let reg = MockAdapterRegistry::new().with_adapter(test_adapter("test/a@1"));
        let mut tx = sample_tx();
        tx.data[0] = 0xff;
        assert_eq!(reg.lookup(&tx), ResolverOutcome::NoMatch);
    }

    #[test]
    fn registry_no_match_when_chain_differs() {
        let reg = MockAdapterRegistry::new().with_adapter(test_adapter("test/a@1"));
        let mut tx = sample_tx();
        tx.chain_id = 137;
        assert_eq!(reg.lookup(&tx), ResolverOutcome::NoMatch);
    }

    #[test]
    fn registry_ambiguous_when_two_adapters_claim_same_key() {
        let reg = MockAdapterRegistry::new()
            .with_adapter(test_adapter("test/a@1"))
            .with_adapter(test_adapter("test/b@1"));
        let outcome = reg.lookup(&sample_tx());
        assert!(matches!(outcome, ResolverOutcome::Ambiguous(_)));
    }

    #[test]
    fn empty_registry_returns_no_match() {
        let reg = MockAdapterRegistry::new();
        assert_eq!(reg.lookup(&sample_tx()), ResolverOutcome::NoMatch);
    }

    #[test]
    fn signature_registry_ambiguous_when_two_adapters_claim_same_key() {
        let reg = MockSignatureRegistry::new()
            .with_adapter(test_signature_adapter("test/sig-a@1"))
            .with_adapter(test_signature_adapter("test/sig-b@1"));

        let outcome = reg.resolve(&sample_sig("Permit"));

        assert!(matches!(outcome, SignatureResolverOutcome::Ambiguous(ids) if ids.len() == 2));
    }
}
