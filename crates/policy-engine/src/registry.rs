//! `TransactionActionAdapter` registry abstractions.
//!
//! - `TransactionActionAdapterRegistry` trait — object-safe contract every registry honors;
//!   `Pipeline` is generic over it so hosts can plug in their own
//!   implementations (caching, hot-reload, remote-mirror, …).
//! - `TransactionResolverOutcome` — what `lookup` / `resolve_with_adapter` returns.
//! - `TransactionActionAdapterIndex` — internal `(chain_id, to, selector)` map shared by
//!   `MockTransactionActionAdapterRegistry` and any future in-memory variants.
//! - `MockTransactionActionAdapterRegistry` — the v0.1 in-memory registry used by tests
//!   and examples.

#[cfg(test)]
use crate::adapter::TransactionMatchKey;
use crate::adapter::{
    ActionAdapterId, SignatureActionAdapter, TransactionActionAdapter,
    TransactionActionAdapterFactory,
};
use crate::core::{Address, ChainId, SignatureRequest, TransactionRequest};
use std::collections::HashMap;
use std::sync::Arc;

type ExactKey = (ChainId, Address, [u8; 4]);
type WildcardTargetKey = (ChainId, [u8; 4]);
type AdapterList = Vec<Arc<dyn TransactionActionAdapter>>;
type SignatureKey = (ChainId, Address, String);

/// Result of resolving a transaction against installed adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionResolverOutcome {
    /// Exactly one adapter matched the (`chain_id`, to, selector) key.
    Resolved(ActionAdapterId),
    /// No adapter matched. The pipeline should emit `LegacyAction::Other`.
    NoMatch,
    /// Two or more adapters matched. v0.1 surfaces the candidate ids; the
    /// pipeline rejects the transaction until the user pins one.
    Ambiguous(Vec<ActionAdapterId>),
}

/// Result of resolving an EIP-712 signature against installed adapters.
pub enum SignatureActionResolverOutcome<'a> {
    /// Exactly one signature adapter matched the
    /// (`chain_id`, `verifyingContract`, `primaryType`) key.
    Resolved(&'a dyn SignatureActionAdapter),
    /// No signature adapter matched. The pipeline should emit
    /// `LegacyAction::Eip712Other`.
    NoMatch,
    /// Two or more signature adapters matched.
    Ambiguous(Vec<ActionAdapterId>),
}

impl std::fmt::Debug for SignatureActionResolverOutcome<'_> {
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
/// The trait is object-safe (`&dyn TransactionActionAdapterRegistry` works), so hosts can swap
/// in remote-cache-backed registries, hot-reload registries, etc., without
/// changing `Pipeline`.
pub trait TransactionActionAdapterRegistry: Send + Sync {
    /// Resolve a transaction to an adapter. When the outcome is `Resolved`,
    /// the second tuple element carries the adapter `Arc` so the caller can
    /// invoke `build` and keep sequencing in the pipeline without a
    /// second lookup. For `NoMatch` / `Ambiguous`, the second element is `None`.
    fn resolve_with_adapter(
        &self,
        tx: &TransactionRequest,
    ) -> (
        TransactionResolverOutcome,
        Option<Arc<dyn TransactionActionAdapter>>,
    );

    /// Convenience: outcome only.
    fn lookup(&self, tx: &TransactionRequest) -> TransactionResolverOutcome {
        self.resolve_with_adapter(tx).0
    }
}

/// Registry for off-chain signature adapters.
pub trait SignatureActionAdapterRegistry: Send + Sync {
    /// Resolve a signature request to a specific adapter.
    fn resolve<'a>(&'a self, sig: &SignatureRequest) -> SignatureActionResolverOutcome<'a>;
}

/// Index over installed adapters, keyed by `(chain_id, to, selector)` plus a
/// wildcard-target bucket for selectors with `to == None` matchers.
#[derive(Default)]
pub struct TransactionActionAdapterIndex {
    exact: HashMap<ExactKey, AdapterList>,
    wildcard_target: HashMap<WildcardTargetKey, AdapterList>,
}

impl std::fmt::Debug for TransactionActionAdapterIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransactionActionAdapterIndex")
            .field("exact_len", &self.exact.len())
            .field("wildcard_target_len", &self.wildcard_target.len())
            .finish()
    }
}

impl TransactionActionAdapterIndex {
    /// Insert all match keys exposed by `adapter`.
    #[allow(clippy::needless_pass_by_value)]
    pub fn insert(&mut self, adapter: Arc<dyn TransactionActionAdapter>) {
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
    pub fn matches_for(&self, tx: &TransactionRequest) -> Vec<Arc<dyn TransactionActionAdapter>> {
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
pub struct MockTransactionActionAdapterRegistry {
    index: TransactionActionAdapterIndex,
}

impl MockTransactionActionAdapterRegistry {
    /// Construct an empty in-memory registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install an adapter. Returns `self` for builder-style chaining.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn with_adapter(mut self, adapter: Arc<dyn TransactionActionAdapter>) -> Self {
        self.index.insert(adapter);
        self
    }

    /// Instantiate and install an adapter from a registry/catalog factory.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn with_factory<F>(self, factory: F) -> Self
    where
        F: TransactionActionAdapterFactory,
    {
        self.with_adapter(factory.create())
    }
}

impl TransactionActionAdapterRegistry for MockTransactionActionAdapterRegistry {
    fn resolve_with_adapter(
        &self,
        tx: &TransactionRequest,
    ) -> (
        TransactionResolverOutcome,
        Option<Arc<dyn TransactionActionAdapter>>,
    ) {
        let matches = self.index.matches_for(tx);
        match matches.len() {
            0 => (TransactionResolverOutcome::NoMatch, None),
            1 => (
                TransactionResolverOutcome::Resolved(matches[0].id()),
                Some(Arc::clone(&matches[0])),
            ),
            _ => {
                let ids = matches.iter().map(|a| a.id()).collect();
                (TransactionResolverOutcome::Ambiguous(ids), None)
            }
        }
    }

    // `lookup` uses the trait's default impl (calls `resolve_with_adapter`).
}

/// In-memory signature adapter registry keyed by EIP-712 match fields.
#[derive(Default)]
pub struct MockSignatureActionAdapterRegistry {
    adapters: Vec<Arc<dyn SignatureActionAdapter>>,
    index: HashMap<SignatureKey, Vec<usize>>,
}

impl std::fmt::Debug for MockSignatureActionAdapterRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockSignatureActionAdapterRegistry")
            .field("adapter_len", &self.adapters.len())
            .field("index_len", &self.index.len())
            .finish()
    }
}

impl MockSignatureActionAdapterRegistry {
    /// Construct an empty in-memory signature registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Install a signature adapter. Returns `self` for builder-style chaining.
    #[must_use]
    #[allow(clippy::needless_pass_by_value)]
    pub fn with_adapter(mut self, adapter: Arc<dyn SignatureActionAdapter>) -> Self {
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

impl SignatureActionAdapterRegistry for MockSignatureActionAdapterRegistry {
    fn resolve<'a>(&'a self, sig: &SignatureRequest) -> SignatureActionResolverOutcome<'a> {
        let key = (
            sig.chain_id,
            sig.typed_data.domain.verifying_contract.clone(),
            normalize_primary_type(&sig.typed_data.primary_type),
        );
        let Some(adapter_indices) = self.index.get(&key) else {
            return SignatureActionResolverOutcome::NoMatch;
        };

        match adapter_indices.as_slice() {
            [] => SignatureActionResolverOutcome::NoMatch,
            [adapter_index] => self
                .adapters
                .get(*adapter_index)
                .map_or(SignatureActionResolverOutcome::NoMatch, |adapter| {
                    SignatureActionResolverOutcome::Resolved(Arc::as_ref(adapter))
                }),
            _ => {
                let ids = adapter_indices
                    .iter()
                    .filter_map(|adapter_index| self.adapters.get(*adapter_index))
                    .map(|adapter| adapter.id())
                    .collect();
                SignatureActionResolverOutcome::Ambiguous(ids)
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
    use crate::adapter::{ActionAdapterError, SignatureMatchKey, TransactionActionAdapter};
    use crate::core::{
        Eip712Domain, Eip712TypedData, LegacyAction, OtherAction, SignatureRequest,
        TransactionRequest,
    };
    use serde_json::json;

    /// Minimal in-crate adapter used only to exercise the registry. Doesn't
    /// touch ABI decoding or oracle data — it just claims a fixed set of
    /// match keys and returns `LegacyAction::Other` from `build`.
    struct TestAdapter {
        id: ActionAdapterId,
        keys: Vec<TransactionMatchKey>,
    }

    struct TestSignatureActionAdapter {
        id: ActionAdapterId,
        keys: Vec<SignatureMatchKey>,
    }

    impl TransactionActionAdapter for TestAdapter {
        fn id(&self) -> ActionAdapterId {
            self.id.clone()
        }
        fn match_keys(&self) -> Vec<TransactionMatchKey> {
            self.keys.clone()
        }
        fn build_action(
            &self,
            tx: &TransactionRequest,
        ) -> Result<LegacyAction, ActionAdapterError> {
            Ok(LegacyAction::Other(OtherAction {
                actor: tx.from.clone(),
                target: tx.to.clone(),
                selector: tx.selector_hex().unwrap_or_else(|| "0x".into()),
                value_wei: tx.value_wei.clone(),
                raw_calldata: format!("0x{}", hex::encode(&tx.data)),
            }))
        }
    }

    impl SignatureActionAdapter for TestSignatureActionAdapter {
        fn id(&self) -> ActionAdapterId {
            self.id.clone()
        }

        fn match_keys(&self) -> Vec<SignatureMatchKey> {
            self.keys.clone()
        }

        fn build_action(
            &self,
            _sig: &SignatureRequest,
        ) -> Result<LegacyAction, ActionAdapterError> {
            Ok(LegacyAction::Other(OtherAction {
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

    fn test_adapter(id: &str) -> Arc<dyn TransactionActionAdapter> {
        Arc::new(TestAdapter {
            id: ActionAdapterId::new(id).expect("static ActionAdapterId is well-formed"),
            keys: vec![TransactionMatchKey::exact(
                1,
                fixed_target(),
                [0xaa, 0xbb, 0xcc, 0xdd],
            )],
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

    fn test_signature_adapter(id: &str) -> Arc<dyn SignatureActionAdapter> {
        Arc::new(TestSignatureActionAdapter {
            id: ActionAdapterId::new(id).expect("static ActionAdapterId is well-formed"),
            keys: vec![SignatureMatchKey::exact(1, fixed_target(), "Permit")],
        })
    }

    #[test]
    fn registry_resolves_single_match() {
        let reg =
            MockTransactionActionAdapterRegistry::new().with_adapter(test_adapter("test/a@1"));
        let outcome = reg.lookup(&sample_tx());
        assert_eq!(
            outcome,
            TransactionResolverOutcome::Resolved(
                ActionAdapterId::new("test/a@1").expect("static ActionAdapterId is well-formed")
            )
        );
    }

    #[test]
    fn registry_no_match_when_target_address_differs() {
        let reg =
            MockTransactionActionAdapterRegistry::new().with_adapter(test_adapter("test/a@1"));
        let mut tx = sample_tx();
        tx.to = Address::new("0x000000000000000000000000000000000000dead").unwrap();
        assert_eq!(reg.lookup(&tx), TransactionResolverOutcome::NoMatch);
    }

    #[test]
    fn registry_no_match_when_selector_differs() {
        let reg =
            MockTransactionActionAdapterRegistry::new().with_adapter(test_adapter("test/a@1"));
        let mut tx = sample_tx();
        tx.data[0] = 0xff;
        assert_eq!(reg.lookup(&tx), TransactionResolverOutcome::NoMatch);
    }

    #[test]
    fn registry_no_match_when_chain_differs() {
        let reg =
            MockTransactionActionAdapterRegistry::new().with_adapter(test_adapter("test/a@1"));
        let mut tx = sample_tx();
        tx.chain_id = 137;
        assert_eq!(reg.lookup(&tx), TransactionResolverOutcome::NoMatch);
    }

    #[test]
    fn registry_ambiguous_when_two_adapters_claim_same_key() {
        let reg = MockTransactionActionAdapterRegistry::new()
            .with_adapter(test_adapter("test/a@1"))
            .with_adapter(test_adapter("test/b@1"));
        let outcome = reg.lookup(&sample_tx());
        assert!(matches!(outcome, TransactionResolverOutcome::Ambiguous(_)));
    }

    #[test]
    fn empty_registry_returns_no_match() {
        let reg = MockTransactionActionAdapterRegistry::new();
        assert_eq!(
            reg.lookup(&sample_tx()),
            TransactionResolverOutcome::NoMatch
        );
    }

    #[test]
    fn signature_registry_ambiguous_when_two_adapters_claim_same_key() {
        let reg = MockSignatureActionAdapterRegistry::new()
            .with_adapter(test_signature_adapter("test/sig-a@1"))
            .with_adapter(test_signature_adapter("test/sig-b@1"));

        let outcome = reg.resolve(&sample_sig("Permit"));

        assert!(
            matches!(outcome, SignatureActionResolverOutcome::Ambiguous(ids) if ids.len() == 2)
        );
    }
}
