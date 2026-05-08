//! `Adapter` trait — the contract every per-protocol calldata mapper
//! implements. Concrete adapter crates live under `crates/adapters/<name>/`
//! and program against [`crate::prelude`].
//!
//! Two responsibilities:
//! - [`Adapter::build`] — protocol-specific decoding: parsed calldata →
//!   semantic [`Action`].

use crate::core::{Action, Address, ChainId, SignatureRequest, TransactionRequest};
use std::sync::Arc;
use thiserror::Error;

/// Stable identifier for an adapter (e.g., `dex-v3/exactInputSingle@0.1.0`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AdapterId {
    raw: String,
    protocol_end: usize,
    name_end: usize,
    version_start: Option<usize>,
}

/// Borrowed parts of an adapter id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AdapterIdParts<'a> {
    /// Protocol namespace, for example `uniswap-v3`.
    pub protocol: &'a str,
    /// Adapter or function name.
    pub name: &'a str,
    /// Optional adapter version component.
    pub version: Option<&'a str>,
}

impl AdapterId {
    /// Parse and store an adapter id.
    ///
    /// # Errors
    ///
    /// Returns an error when the id does not match
    /// `<protocol>/<name>[@<version>]`.
    pub fn new(s: &str) -> Result<Self, AdapterIdError> {
        let parsed = parse_adapter_id(s)?;
        Ok(Self {
            raw: s.to_string(),
            protocol_end: parsed.protocol_end,
            name_end: parsed.name_end,
            version_start: parsed.version_start,
        })
    }

    /// Parse an adapter id without allocating and return borrowed parts.
    ///
    /// # Errors
    ///
    /// Returns an error when the id does not match
    /// `<protocol>/<name>[@<version>]`.
    pub fn parts(s: &str) -> Result<AdapterIdParts<'_>, AdapterIdError> {
        let parsed = parse_adapter_id(s)?;
        Ok(AdapterIdParts {
            protocol: &s[..parsed.protocol_end],
            name: &s[(parsed.protocol_end + 1)..parsed.name_end],
            version: parsed.version_start.map(|start| &s[start..]),
        })
    }

    /// Adapter protocol namespace.
    #[must_use]
    pub fn protocol(&self) -> &str {
        &self.raw[..self.protocol_end]
    }

    /// Adapter name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.raw[(self.protocol_end + 1)..self.name_end]
    }

    /// Optional version string.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.version_start.map(|start| &self.raw[start..])
    }

    /// Original adapter id string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

/// Error returned when parsing an adapter id fails.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AdapterIdError {
    /// The id was empty.
    #[error("adapter id is empty")]
    Empty,
    /// The id did not contain the protocol/name separator.
    #[error("adapter id has missing protocol separator: expected <protocol>/<name>[@<version>]")]
    MissingSeparator,
    /// The protocol component was empty.
    #[error("adapter id must include a protocol")]
    MissingProtocol,
    /// The name component was empty.
    #[error("adapter id must include a name")]
    MissingName,
    /// The version marker was present but empty.
    #[error("adapter id has an empty version component")]
    EmptyVersion,
    /// The version component contained unsupported characters.
    #[error("adapter id has an invalid version '{version}'")]
    InvalidVersion {
        /// Invalid version text.
        version: String,
    },
}

#[derive(Debug, Clone, Copy)]
struct ParsedAdapterId {
    protocol_end: usize,
    name_end: usize,
    version_start: Option<usize>,
}

fn parse_adapter_id(s: &str) -> Result<ParsedAdapterId, AdapterIdError> {
    if s.is_empty() {
        return Err(AdapterIdError::Empty);
    }

    let Some((protocol, tail)) = s.split_once('/') else {
        return Err(AdapterIdError::MissingSeparator);
    };
    if protocol.is_empty() {
        return Err(AdapterIdError::MissingProtocol);
    }

    if tail.is_empty() {
        return Err(AdapterIdError::MissingName);
    }

    let protocol_len = protocol.len();
    let (name, version_start) = if let Some((name, version)) = tail.split_once('@') {
        if name.is_empty() {
            return Err(AdapterIdError::MissingName);
        }
        if version.is_empty() {
            return Err(AdapterIdError::EmptyVersion);
        }
        if !is_valid_version(version) {
            return Err(AdapterIdError::InvalidVersion {
                version: version.to_string(),
            });
        }
        (name, Some(protocol_len + 1 + name.len() + 1))
    } else {
        (tail, None)
    };

    let name_end = protocol_len + 1 + name.len();
    let name_start = protocol_len + 1;
    if name_start >= name_end {
        return Err(AdapterIdError::MissingName);
    }

    Ok(ParsedAdapterId {
        protocol_end: protocol_len,
        name_end,
        version_start,
    })
}

fn is_valid_version(version: &str) -> bool {
    version.split('.').all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    })
}

/// Error returned by an adapter while decoding or lowering a transaction.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AdapterError {
    /// Calldata did not match the adapter's expected ABI shape.
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

/// A single `(chain_id, verifying_contract, primary_type)` signature pattern
/// matched by a signature adapter.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignatureMatchKey {
    /// EVM chain id supplied by the wallet request.
    pub chain_id: ChainId,
    /// EIP-712 verifying contract.
    pub verifying_contract: Address,
    /// EIP-712 primary type.
    ///
    /// Signature registry matching is case-insensitive because EIP-712 does
    /// not strictly mandate primary-type casing. The original casing remains
    /// preserved in `context.base.primaryType` for observability and for hosts
    /// that want to enforce exact casing with Cedar policy.
    pub primary_type: String,
}

impl SignatureMatchKey {
    /// Exact signature matcher.
    #[must_use]
    pub fn exact(
        chain_id: ChainId,
        verifying_contract: Address,
        primary_type: impl Into<String>,
    ) -> Self {
        Self {
            chain_id,
            verifying_contract,
            primary_type: primary_type.into(),
        }
    }
}

/// Adapter surface for off-chain EIP-712 signature requests.
pub trait SignatureAdapter: Send + Sync {
    /// Stable adapter id.
    fn id(&self) -> AdapterId;

    /// The set of `(chain_id, verifying_contract, primary_type)` keys this
    /// adapter wants to match.
    fn match_keys(&self) -> Vec<SignatureMatchKey>;

    /// Try to construct an `Action` from this signature request.
    ///
    /// # Errors
    ///
    /// Returns an error when typed-data decoding or mapping fails.
    fn build(&self, sig: &SignatureRequest) -> Result<Action, AdapterError>;
}

/// Internal helper surface shared by first-party signature adapter crates.
#[doc(hidden)]
pub mod signature_helpers {
    use super::{AdapterError, AdapterId};
    use crate::core::{Address, ChainId, Token};
    use alloy_primitives::U256;
    use serde_json::{Map, Value};
    use std::collections::HashMap;

    /// Token metadata lookup keyed by `(chain_id, address)`.
    #[derive(Debug, Clone, Default)]
    pub struct TokenLookup {
        tokens: HashMap<(ChainId, String), Token>,
    }

    impl TokenLookup {
        /// Construct an empty token lookup.
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        /// Construct a lookup pre-populated with `tokens`.
        #[must_use]
        pub fn with_tokens<I>(tokens: I) -> Self
        where
            I: IntoIterator<Item = Token>,
        {
            let mut lookup = Self::new();
            for token in tokens {
                lookup.add(token);
            }
            lookup
        }

        /// Add or replace token metadata.
        pub fn add(&mut self, token: Token) {
            self.tokens.insert(
                (token.chain_id, token.address.as_str().to_lowercase()),
                token,
            );
        }

        /// Return metadata for `address`, defaulting to an UNKNOWN 18-decimal
        /// ERC-20 shape when no metadata is installed.
        #[must_use]
        pub fn get(&self, chain_id: ChainId, address: &Address) -> Token {
            self.tokens
                .get(&(chain_id, address.as_str().to_lowercase()))
                .cloned()
                .unwrap_or_else(|| Token {
                    chain_id,
                    address: address.clone(),
                    symbol: "UNKNOWN".into(),
                    decimals: 18,
                    is_native: false,
                })
        }

        /// Return `(chain_id, verifying_contract)` match targets.
        #[must_use]
        pub fn targets(&self) -> Vec<(ChainId, Address)> {
            self.tokens
                .values()
                .map(|token| (token.chain_id, token.address.clone()))
                .collect()
        }
    }

    /// Build a static token and panic if the checked-in address is invalid.
    #[must_use]
    pub fn static_token(chain_id: ChainId, address: &str, symbol: &str, decimals: u32) -> Token {
        Token {
            chain_id,
            address: Address::new(address).unwrap_or_else(|err| {
                panic_static(&format!("invalid static token address {address}: {err}"))
            }),
            symbol: symbol.into(),
            decimals,
            is_native: false,
        }
    }

    /// Borrow a JSON object.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::BadCalldata`] when `value` is not an object.
    pub fn object<'a>(
        value: &'a Value,
        label: &str,
    ) -> Result<&'a Map<String, Value>, AdapterError> {
        value
            .as_object()
            .ok_or_else(|| AdapterError::BadCalldata(format!("{label} must be an object")))
    }

    /// Borrow a JSON object field.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::BadCalldata`] when `field` is missing or not an
    /// object.
    pub fn object_field<'a>(
        object: &'a Map<String, Value>,
        field: &str,
    ) -> Result<&'a Map<String, Value>, AdapterError> {
        object
            .get(field)
            .ok_or_else(|| AdapterError::BadCalldata(format!("missing field {field}")))
            .and_then(|value| self::object(value, field))
    }

    /// Borrow a JSON array field.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::BadCalldata`] when `field` is missing or not an
    /// array.
    pub fn array_field<'a>(
        object: &'a Map<String, Value>,
        field: &str,
    ) -> Result<&'a [Value], AdapterError> {
        object
            .get(field)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .ok_or_else(|| AdapterError::BadCalldata(format!("{field} must be an array")))
    }

    /// Parse an address field.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::BadCalldata`] when the field is missing,
    /// non-stringish, or not an EVM address.
    pub fn address_field(
        object: &Map<String, Value>,
        field: &str,
    ) -> Result<Address, AdapterError> {
        let value = stringish_field(object, field)?;
        Address::new(&value).map_err(AdapterError::BadCalldata)
    }

    /// Parse a u64 field encoded as a JSON string or number.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::BadCalldata`] when the field is missing, not a
    /// uint256 decimal, or does not fit in u64.
    pub fn u64_field(object: &Map<String, Value>, field: &str) -> Result<u64, AdapterError> {
        let value = u256_string_field(object, field)?;
        value
            .parse::<u64>()
            .map_err(|err| AdapterError::BadCalldata(format!("{field} does not fit u64: {err}")))
    }

    /// Parse and normalize a uint256 decimal field.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::BadCalldata`] when the field is missing,
    /// non-stringish, or not a uint256 decimal.
    pub fn u256_string_field(
        object: &Map<String, Value>,
        field: &str,
    ) -> Result<String, AdapterError> {
        let value = stringish_field(object, field)?;
        U256::from_str_radix(&value, 10)
            .map(|parsed| parsed.to_string())
            .map_err(|err| AdapterError::BadCalldata(format!("{field} must be uint256: {err}")))
    }

    /// Return a string value from a JSON string or number field.
    ///
    /// # Errors
    ///
    /// Returns [`AdapterError::BadCalldata`] when the field is missing or not
    /// a string/number.
    pub fn stringish_field(
        object: &Map<String, Value>,
        field: &str,
    ) -> Result<String, AdapterError> {
        let value = object
            .get(field)
            .ok_or_else(|| AdapterError::BadCalldata(format!("missing field {field}")))?;
        match value {
            Value::String(s) => Ok(s.clone()),
            Value::Number(n) => Ok(n.to_string()),
            _ => Err(AdapterError::BadCalldata(format!(
                "{field} must be a string or number"
            ))),
        }
    }

    /// Parse a static adapter id and panic if it is malformed.
    #[must_use]
    #[allow(clippy::panic)]
    pub fn static_adapter_id(raw: &str) -> AdapterId {
        AdapterId::new(raw).unwrap_or_else(|err| panic!("invalid static adapter id {raw}: {err}"))
    }

    /// Panic for malformed checked-in constants.
    #[allow(clippy::panic)]
    pub fn panic_static(message: &str) -> ! {
        panic!("{message}");
    }
}

/// Semantic action families an adapter may emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    /// DEX action family.
    Dex,
    /// Fallback unknown action family.
    Other,
}

/// Solidity function surface covered by an adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolidityFunction {
    /// Solidity function name.
    pub name: String,
    /// Canonical Solidity function signature.
    pub signature: String,
    /// Four-byte ABI selector.
    pub selector: [u8; 4],
}

/// Compile-time-friendly Solidity function descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolidityFunctionSpec {
    /// Solidity function name.
    pub name: &'static str,
    /// Canonical Solidity function signature.
    pub signature: &'static str,
    /// Four-byte ABI selector.
    pub selector: [u8; 4],
}

impl SolidityFunctionSpec {
    /// Construct a compile-time function descriptor.
    #[must_use]
    pub const fn new(name: &'static str, signature: &'static str, selector: [u8; 4]) -> Self {
        Self {
            name,
            signature,
            selector,
        }
    }

    /// Convert into an owned descriptor for registry metadata.
    #[must_use]
    pub fn into_owned_function(self) -> SolidityFunction {
        SolidityFunction::new(self.name, self.signature, self.selector)
    }
}

impl SolidityFunction {
    /// Construct an owned Solidity function descriptor.
    #[must_use]
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
    /// EVM chain id.
    pub chain_id: ChainId,
    /// Contract address.
    pub address: Address,
}

impl ContractTarget {
    /// Construct a contract target.
    #[must_use]
    pub const fn new(chain_id: ChainId, address: Address) -> Self {
        Self { chain_id, address }
    }

    /// Build a match key for `selector` at this target.
    #[must_use]
    pub fn match_key(&self, selector: [u8; 4]) -> MatchKey {
        MatchKey::exact(self.chain_id, self.address.clone(), selector)
    }
}

/// Static-ish metadata a registry can index before invoking an adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterDescriptor {
    /// Stable adapter id.
    pub id: AdapterId,
    /// Protocol id used in policy context.
    pub protocol_id: String,
    /// Adapter shape.
    pub kind: AdapterKind,
    /// Solidity functions covered by the adapter.
    pub functions: Vec<SolidityFunction>,
    /// Registry match keys covered by the adapter.
    pub match_keys: Vec<MatchKey>,
    /// Action kinds emitted by the adapter.
    pub emitted_actions: Vec<ActionKind>,
}

impl AdapterDescriptor {
    /// Construct adapter metadata.
    #[must_use]
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

    /// Build a minimal descriptor from a runtime adapter instance.
    pub fn from_adapter(adapter: &dyn Adapter) -> Self {
        let id = adapter.id();
        let protocol_id = id.protocol().to_string();
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
    /// Registry-visible metadata for this factory.
    fn descriptor(&self) -> AdapterDescriptor;
    /// Instantiate the adapter.
    fn create(&self) -> Arc<dyn Adapter>;
}

/// Function pointer used by static adapter factories.
pub type AdapterConstructor = fn() -> Arc<dyn Adapter>;

/// Static factory for adapters that can be constructed with `Default`.
#[derive(Debug, Clone)]
pub struct StaticAdapterFactory {
    descriptor: AdapterDescriptor,
    constructor: AdapterConstructor,
}

impl StaticAdapterFactory {
    /// Construct a factory from metadata and a constructor function.
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
    /// Stable adapter id.
    const ADAPTER_ID: &'static str;
    /// Protocol id emitted into DEX context.
    const PROTOCOL_ID: &'static str;
    /// Adapter shape.
    const KIND: AdapterKind;
    /// Solidity functions covered by this adapter.
    const FUNCTIONS: &'static [SolidityFunctionSpec];
    /// Semantic action families this adapter may emit.
    const EMITTED_ACTIONS: &'static [ActionKind];

    /// Chain-specific contracts this adapter targets.
    fn contract_targets(&self) -> Vec<ContractTarget>;

    /// Build a semantic action from a transaction.
    ///
    /// # Errors
    ///
    /// Returns an error when calldata cannot be decoded or mapped.
    fn build_action(&self, tx: &TransactionRequest) -> Result<Action, AdapterError>;

    /// Parsed static adapter id.
    #[allow(clippy::expect_used)]
    #[must_use]
    fn adapter_id() -> AdapterId {
        AdapterId::new(Self::ADAPTER_ID).expect("static AdapterId is well-formed")
    }

    /// Match keys generated from contract targets and functions.
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

    /// Registry descriptor generated from typed adapter constants.
    fn typed_descriptor(&self) -> AdapterDescriptor {
        let id = Self::adapter_id();
        let protocol_id = id.protocol().to_string();
        AdapterDescriptor::new(
            id,
            &protocol_id,
            Self::KIND,
            Self::FUNCTIONS
                .iter()
                .map(|f| f.into_owned_function())
                .collect(),
            self.typed_match_keys(),
            Self::EMITTED_ACTIONS.to_vec(),
        )
    }

    /// Static factory for this typed adapter.
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
}

/// One adapter handles one (or a small set of) `(chain_id, to, selector)` keys
/// and emits an `Action` from a decoded `TransactionRequest`.
pub trait Adapter: Send + Sync {
    /// Stable adapter id.
    fn id(&self) -> AdapterId;

    /// The set of `(chain_id, to, selector)` keys this adapter wants to match.
    /// `to == None` means "any contract address".
    fn match_keys(&self) -> Vec<MatchKey>;

    /// Registry/catalog metadata. Simple adapters can use the default; richer
    /// adapters should override it with function signatures and action kinds.
    fn descriptor(&self) -> AdapterDescriptor {
        let id = self.id();
        let protocol_id = id.protocol().to_string();
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
    /// # Errors
    ///
    /// Returns an error when calldata cannot be decoded or mapped.
    fn build(&self, tx: &TransactionRequest) -> Result<Action, AdapterError>;
}

/// A single `(chain_id, to, selector)` pattern an adapter matches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchKey {
    /// EVM chain id.
    pub chain_id: ChainId,
    /// `None` represents the wildcard (`"*"` in the manifest spec).
    pub to: Option<Address>,
    /// Four-byte function selector.
    pub selector: [u8; 4],
}

impl MatchKey {
    /// Exact target-address matcher.
    #[must_use]
    pub const fn exact(chain_id: ChainId, to: Address, selector: [u8; 4]) -> Self {
        Self {
            chain_id,
            to: Some(to),
            selector,
        }
    }

    /// Wildcard target-address matcher.
    #[must_use]
    pub const fn wildcard_target(chain_id: ChainId, selector: [u8; 4]) -> Self {
        Self {
            chain_id,
            to: None,
            selector,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::OtherAction;

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
            Ok(Action::Other(OtherAction {
                actor: tx.from.clone(),
                target: tx.to.clone(),
                selector: "0xaabbccdd".into(),
                value_wei: tx.value_wei.clone(),
                raw_calldata: hex::encode(&tx.data),
            }))
        }
    }

    #[test]
    fn typed_adapter_supplies_runtime_adapter_contract() {
        let adapter = TypedNoopAdapter;
        let as_adapter: &dyn Adapter = &adapter;
        let target = Address::new("0x1111111111111111111111111111111111111111").unwrap();

        assert_eq!(
            as_adapter.id(),
            AdapterId::new("test/typed-noop@0.0.1").expect("static AdapterId is well-formed")
        );
        assert_eq!(
            as_adapter.match_keys(),
            vec![MatchKey::exact(1, target, [0xaa, 0xbb, 0xcc, 0xdd])]
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
            AdapterId::new("test/typed-noop@0.0.1").expect("static AdapterId is well-formed")
        );
        assert_eq!(
            factory.create().id(),
            AdapterId::new("test/typed-noop@0.0.1").expect("static AdapterId is well-formed")
        );
    }
}
