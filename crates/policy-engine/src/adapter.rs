//! `Adapter` trait — the contract every per-protocol calldata mapper
//! implements. Concrete adapter crates live under `crates/adapters/<name>/`
//! and program against [`crate::prelude`].
//!
//! Two responsibilities:
//! - [`Adapter::build`] — protocol-specific decoding: parsed calldata →
//!   semantic [`Action`].
//! - [`Adapter::into_request`] — full "calldata → Cedar `PolicyRequest`"
//!   lowering. Default impl chains `build` → [`crate::lowering::enrich_with_usd`]
//!   → [`crate::lowering::request_from_action`]. Override only if you need a
//!   custom request shape (e.g., to skip the `Action` intermediate entirely).

use crate::core::{Action, Address, ChainId, TransactionRequest};
use crate::lowering::{enrich_request_with_capabilities, enrich_with_usd, request_from_action};
use crate::host::HostCapabilities;
use crate::policy::PolicyRequest;
use std::sync::Arc;
use thiserror::Error;

/// Stable identifier for an adapter (e.g., `uniswap-v3/exactInputSingle@0.1.0`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdapterId(pub String);

impl AdapterId {
    pub fn new(s: &str) -> Self {
        AdapterId(s.into())
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum AdapterError {
    #[error("adapter cannot decode this calldata: {0}")]
    BadCalldata(String),
}

/// Coarse adapter shape for registry/catalog UIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterKind {
    /// One Solidity function maps to one adapter module.
    Function,
    /// One router function contains nested semantic calls.
    CompositeRouter,
}

/// Semantic action families an adapter may emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Swap,
    Multi,
    Other,
}

/// Solidity function surface covered by an adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolidityFunction {
    pub name: String,
    pub signature: String,
    pub selector: [u8; 4],
}

/// Compile-time-friendly Solidity function descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolidityFunctionSpec {
    pub name: &'static str,
    pub signature: &'static str,
    pub selector: [u8; 4],
}

impl SolidityFunctionSpec {
    pub const fn new(name: &'static str, signature: &'static str, selector: [u8; 4]) -> Self {
        Self {
            name,
            signature,
            selector,
        }
    }

    pub fn into_owned_function(self) -> SolidityFunction {
        SolidityFunction::new(self.name, self.signature, self.selector)
    }
}

impl SolidityFunction {
    pub fn new(name: &str, signature: &str, selector: [u8; 4]) -> Self {
        Self {
            name: name.into(),
            signature: signature.into(),
            selector,
        }
    }
}

/// A chain-specific contract address an adapter targets.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContractTarget {
    pub chain_id: ChainId,
    pub address: Address,
}

impl ContractTarget {
    pub fn new(chain_id: ChainId, address: Address) -> Self {
        Self { chain_id, address }
    }

    pub fn match_key(&self, selector: [u8; 4]) -> MatchKey {
        MatchKey::exact(self.chain_id, self.address.clone(), selector)
    }
}

/// Static-ish metadata a registry can index before invoking an adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterDescriptor {
    pub id: AdapterId,
    pub protocol_id: String,
    pub kind: AdapterKind,
    pub functions: Vec<SolidityFunction>,
    pub match_keys: Vec<MatchKey>,
    pub emitted_actions: Vec<ActionKind>,
}

impl AdapterDescriptor {
    pub fn new(
        id: AdapterId,
        protocol_id: &str,
        kind: AdapterKind,
        functions: Vec<SolidityFunction>,
        match_keys: Vec<MatchKey>,
        emitted_actions: Vec<ActionKind>,
    ) -> Self {
        Self {
            id,
            protocol_id: protocol_id.into(),
            kind,
            functions,
            match_keys,
            emitted_actions,
        }
    }

    pub fn from_adapter(adapter: &dyn Adapter) -> Self {
        let id = adapter.id();
        let protocol_id =
            id.0.split('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown")
                .to_string();
        Self {
            id,
            protocol_id,
            kind: AdapterKind::Function,
            functions: Vec::new(),
            match_keys: adapter.match_keys(),
            emitted_actions: Vec::new(),
        }
    }
}

fn construct_typed_adapter<T: TypedAdapter>() -> Arc<dyn Adapter> {
    Arc::new(T::default())
}

/// Factory surface a remote/local adapter registry can use to instantiate an
/// adapter after it has matched the descriptor.
pub trait AdapterFactory: Send + Sync {
    fn descriptor(&self) -> AdapterDescriptor;
    fn create(&self) -> Arc<dyn Adapter>;
}

pub type AdapterConstructor = fn() -> Arc<dyn Adapter>;

#[derive(Clone)]
pub struct StaticAdapterFactory {
    descriptor: AdapterDescriptor,
    constructor: AdapterConstructor,
}

impl StaticAdapterFactory {
    pub fn new(descriptor: AdapterDescriptor, constructor: AdapterConstructor) -> Self {
        Self {
            descriptor,
            constructor,
        }
    }
}

impl AdapterFactory for StaticAdapterFactory {
    fn descriptor(&self) -> AdapterDescriptor {
        self.descriptor.clone()
    }

    fn create(&self) -> Arc<dyn Adapter> {
        (self.constructor)()
    }
}

/// Typed authoring surface for third-party adapters.
///
/// Implement this trait when one crate/module owns one logical adapter. The
/// associated constants describe the registry-visible surface; `build_action`
/// is the only protocol-specific runtime function simple adapters must provide.
pub trait TypedAdapter: Send + Sync + Default + Sized + 'static {
    const ADAPTER_ID: &'static str;
    const PROTOCOL_ID: &'static str;
    const KIND: AdapterKind;
    const FUNCTIONS: &'static [SolidityFunctionSpec];
    const EMITTED_ACTIONS: &'static [ActionKind];

    fn contract_targets(&self) -> Vec<ContractTarget>;

    fn build_action(&self, tx: &TransactionRequest) -> Result<Action, AdapterError>;

    fn build_leaf_actions(&self, tx: &TransactionRequest) -> Result<Vec<Action>, AdapterError> {
        Ok(vec![self.build_action(tx)?])
    }

    #[allow(clippy::wrong_self_convention)]
    fn lower_requests(
        &self,
        tx: &TransactionRequest,
        host: &HostCapabilities,
    ) -> Result<Vec<PolicyRequest>, AdapterError> {
        let mut actions = self.build_leaf_actions(tx)?;
        let mut requests = Vec::with_capacity(actions.len());
        for action in &mut actions {
            enrich_with_usd(action, host.oracle());
            let mut req = request_from_action(action);
            enrich_request_with_capabilities(&mut req, action, host);
            requests.push(req);
        }
        Ok(requests)
    }

    fn adapter_id() -> AdapterId {
        AdapterId::new(Self::ADAPTER_ID)
    }

    fn typed_match_keys(&self) -> Vec<MatchKey> {
        let targets = self.contract_targets();
        targets
            .iter()
            .flat_map(|target| {
                Self::FUNCTIONS
                    .iter()
                    .map(|function| target.match_key(function.selector))
            })
            .collect()
    }

    fn typed_descriptor(&self) -> AdapterDescriptor {
        AdapterDescriptor::new(
            Self::adapter_id(),
            Self::PROTOCOL_ID,
            Self::KIND,
            Self::FUNCTIONS
                .iter()
                .map(|f| f.into_owned_function())
                .collect(),
            self.typed_match_keys(),
            Self::EMITTED_ACTIONS.to_vec(),
        )
    }

    fn factory() -> StaticAdapterFactory {
        StaticAdapterFactory::new(
            Self::default().typed_descriptor(),
            construct_typed_adapter::<Self>,
        )
    }
}

impl<T> Adapter for T
where
    T: TypedAdapter,
{
    fn id(&self) -> AdapterId {
        T::adapter_id()
    }

    fn match_keys(&self) -> Vec<MatchKey> {
        self.typed_match_keys()
    }

    fn descriptor(&self) -> AdapterDescriptor {
        self.typed_descriptor()
    }

    fn build(&self, tx: &TransactionRequest) -> Result<Action, AdapterError> {
        self.build_action(tx)
    }

    fn build_actions(&self, tx: &TransactionRequest) -> Result<Vec<Action>, AdapterError> {
        self.build_leaf_actions(tx)
    }

    fn into_requests(
        &self,
        tx: &TransactionRequest,
        host: &HostCapabilities,
    ) -> Result<Vec<PolicyRequest>, AdapterError> {
        self.lower_requests(tx, host)
    }
}

/// One adapter handles one (or a small set of) `(chain_id, to, selector)` keys
/// and emits an `Action` from a decoded `TransactionRequest`.
pub trait Adapter: Send + Sync {
    fn id(&self) -> AdapterId;

    /// The set of `(chain_id, to, selector)` keys this adapter wants to match.
    /// `to == None` means "any contract address".
    fn match_keys(&self) -> Vec<MatchKey>;

    /// Registry/catalog metadata. Simple adapters can use the default; richer
    /// adapters should override it with function signatures and action kinds.
    fn descriptor(&self) -> AdapterDescriptor {
        let id = self.id();
        let protocol_id =
            id.0.split('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown")
                .to_string();
        AdapterDescriptor {
            id,
            protocol_id,
            kind: AdapterKind::Function,
            functions: Vec::new(),
            match_keys: self.match_keys(),
            emitted_actions: Vec::new(),
        }
    }

    /// Try to construct an `Action` from this transaction. Called only after
    /// the resolver has selected this adapter, so the implementation may
    /// assume the calldata starts with the matching selector.
    fn build(&self, tx: &TransactionRequest) -> Result<Action, AdapterError>;

    /// Construct zero or more semantic leaf actions. Simple adapters keep the
    /// default one-action behavior; composite routers override this or
    /// `into_requests` directly.
    fn build_actions(&self, tx: &TransactionRequest) -> Result<Vec<Action>, AdapterError> {
        Ok(vec![self.build(tx)?])
    }

    /// Default lowering: `build` → `enrich_with_usd` → `request_from_action`.
    #[allow(clippy::wrong_self_convention)]
    fn into_request(
        &self,
        tx: &TransactionRequest,
        host: &HostCapabilities,
    ) -> Result<PolicyRequest, AdapterError> {
        let mut action = self.build(tx)?;
        enrich_with_usd(&mut action, host.oracle());
        let mut req = request_from_action(&action);
        enrich_request_with_capabilities(&mut req, &action, host);
        Ok(req)
    }

    /// Multi-request lowering used by the pipeline. The default delegates to
    /// `into_request` so adapters that override the single-request path remain
    /// backward compatible. Composite adapters should override this method.
    #[allow(clippy::wrong_self_convention)]
    fn into_requests(
        &self,
        tx: &TransactionRequest,
        host: &HostCapabilities,
    ) -> Result<Vec<PolicyRequest>, AdapterError> {
        Ok(vec![self.into_request(tx, host)?])
    }
}

/// A single `(chain_id, to, selector)` pattern an adapter matches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchKey {
    pub chain_id: ChainId,
    /// `None` represents the wildcard (`"*"` in the manifest spec).
    pub to: Option<Address>,
    pub selector: [u8; 4],
}

impl MatchKey {
    pub fn exact(chain_id: ChainId, to: Address, selector: [u8; 4]) -> Self {
        MatchKey {
            chain_id,
            to: Some(to),
            selector,
        }
    }

    pub fn wildcard_target(chain_id: ChainId, selector: [u8; 4]) -> Self {
        MatchKey {
            chain_id,
            to: None,
            selector,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct TypedNoopAdapter;

    impl TypedAdapter for TypedNoopAdapter {
        const ADAPTER_ID: &'static str = "test/typed-noop@0.0.1";
        const PROTOCOL_ID: &'static str = "test";
        const KIND: AdapterKind = AdapterKind::Function;
        const FUNCTIONS: &'static [SolidityFunctionSpec] = &[SolidityFunctionSpec::new(
            "noop",
            "noop(uint256)",
            [0xaa, 0xbb, 0xcc, 0xdd],
        )];
        const EMITTED_ACTIONS: &'static [ActionKind] = &[ActionKind::Other];

        fn contract_targets(&self) -> Vec<ContractTarget> {
            vec![ContractTarget::new(
                1,
                Address::new("0x1111111111111111111111111111111111111111").unwrap(),
            )]
        }

        fn build_action(&self, tx: &TransactionRequest) -> Result<Action, AdapterError> {
            Ok(Action::Other {
                actor: tx.from.clone(),
                target: tx.to.clone(),
                selector: "0xaabbccdd".into(),
                value_wei: tx.value_wei.clone(),
                raw_calldata: hex::encode(&tx.data),
            })
        }
    }

    #[test]
    fn typed_adapter_supplies_runtime_adapter_contract() {
        let adapter = TypedNoopAdapter;
        let as_adapter: &dyn Adapter = &adapter;
        let target = Address::new("0x1111111111111111111111111111111111111111").unwrap();

        assert_eq!(as_adapter.id(), AdapterId::new("test/typed-noop@0.0.1"));
        assert_eq!(
            as_adapter.match_keys(),
            vec![MatchKey::exact(1, target.clone(), [0xaa, 0xbb, 0xcc, 0xdd])]
        );

        let descriptor = as_adapter.descriptor();
        assert_eq!(descriptor.protocol_id, "test");
        assert_eq!(descriptor.kind, AdapterKind::Function);
        assert_eq!(descriptor.functions[0].signature, "noop(uint256)");
        assert_eq!(descriptor.emitted_actions, vec![ActionKind::Other]);
    }

    #[test]
    fn typed_adapter_factory_instantiates_adapter() {
        let factory = TypedNoopAdapter::factory();
        assert_eq!(
            factory.descriptor().id,
            AdapterId::new("test/typed-noop@0.0.1")
        );
        assert_eq!(
            factory.create().id(),
            AdapterId::new("test/typed-noop@0.0.1")
        );
    }
}
