//! Action adapter traits.
//!
//! This is the contract every per-protocol transaction or signature mapper
//! implements. Concrete adapter crates live under `crates/adapters/<name>/`
//! and program against [`crate::prelude`].
//!
//! Two responsibilities:
//! - [`TransactionActionAdapter::build_action`] — parsed calldata → semantic [`LegacyAction`].
//! - [`SignatureActionAdapter::build_action`] — parsed typed data → semantic [`LegacyAction`].

use crate::core::{Address, ChainId, LegacyAction, SignatureRequest, TransactionRequest};
use std::sync::Arc;
use thiserror::Error;

/// Stable identifier for an adapter (e.g., `dex-v3/exactInputSingle@0.1.0`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionAdapterId {
    raw: String,
    protocol_end: usize,
    name_end: usize,
    version_start: Option<usize>,
}

/// Borrowed parts of an adapter id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActionAdapterIdParts<'a> {
    /// Protocol namespace, for example `uniswap-v3`.
    pub protocol: &'a str,
    /// Action adapter or function name.
    pub name: &'a str,
    /// Optional adapter version component.
    pub version: Option<&'a str>,
}

impl ActionAdapterId {
    /// Parse and store an adapter id.
    ///
    /// # Errors
    ///
    /// Returns an error when the id does not match
    /// `<protocol>/<name>[@<version>]`.
    pub fn new(s: &str) -> Result<Self, ActionAdapterIdError> {
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
    pub fn parts(s: &str) -> Result<ActionAdapterIdParts<'_>, ActionAdapterIdError> {
        let parsed = parse_adapter_id(s)?;
        Ok(ActionAdapterIdParts {
            protocol: &s[..parsed.protocol_end],
            name: &s[(parsed.protocol_end + 1)..parsed.name_end],
            version: parsed.version_start.map(|start| &s[start..]),
        })
    }

    /// Action adapter protocol namespace.
    #[must_use]
    pub fn protocol(&self) -> &str {
        &self.raw[..self.protocol_end]
    }

    /// Action adapter name.
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
pub enum ActionAdapterIdError {
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
struct ParsedActionAdapterId {
    protocol_end: usize,
    name_end: usize,
    version_start: Option<usize>,
}

fn parse_adapter_id(s: &str) -> Result<ParsedActionAdapterId, ActionAdapterIdError> {
    if s.is_empty() {
        return Err(ActionAdapterIdError::Empty);
    }

    let Some((protocol, tail)) = s.split_once('/') else {
        return Err(ActionAdapterIdError::MissingSeparator);
    };
    if protocol.is_empty() {
        return Err(ActionAdapterIdError::MissingProtocol);
    }

    if tail.is_empty() {
        return Err(ActionAdapterIdError::MissingName);
    }

    let protocol_len = protocol.len();
    let (name, version_start) = if let Some((name, version)) = tail.split_once('@') {
        if name.is_empty() {
            return Err(ActionAdapterIdError::MissingName);
        }
        if version.is_empty() {
            return Err(ActionAdapterIdError::EmptyVersion);
        }
        if !is_valid_version(version) {
            return Err(ActionAdapterIdError::InvalidVersion {
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
        return Err(ActionAdapterIdError::MissingName);
    }

    Ok(ParsedActionAdapterId {
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
pub enum ActionAdapterError {
    /// Calldata did not match the adapter's expected ABI shape.
    #[error("adapter cannot decode this calldata: {0}")]
    BadCalldata(String),
}

/// Coarse adapter shape for registry/catalog UIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionActionAdapterKind {
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

/// Action adapter surface for off-chain EIP-712 signature requests.
pub trait SignatureActionAdapter: Send + Sync {
    /// Stable adapter id.
    fn id(&self) -> ActionAdapterId;

    /// The set of `(chain_id, verifying_contract, primary_type)` keys this
    /// adapter wants to match.
    fn match_keys(&self) -> Vec<SignatureMatchKey>;

    /// Registry/catalog metadata. Simple adapters can use the default; richer
    /// adapters should override it with emitted action kinds.
    fn descriptor(&self) -> SignatureActionAdapterDescriptor {
        let id = self.id();
        let protocol_id = id.protocol().to_string();
        SignatureActionAdapterDescriptor {
            id,
            protocol_id,
            match_keys: self.match_keys(),
            emitted_actions: Vec::new(),
        }
    }

    /// Try to construct a `LegacyAction` from this signature request.
    ///
    /// # Errors
    ///
    /// Returns an error when typed-data decoding or mapping fails.
    fn build_action(&self, sig: &SignatureRequest) -> Result<LegacyAction, ActionAdapterError>;
}

/// Internal helper surface shared by first-party signature adapter crates.
#[doc(hidden)]
pub mod signature_helpers {
    use super::{ActionAdapterError, ActionAdapterId};
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
    /// Returns [`ActionAdapterError::BadCalldata`] when `value` is not an object.
    pub fn object<'a>(
        value: &'a Value,
        label: &str,
    ) -> Result<&'a Map<String, Value>, ActionAdapterError> {
        value
            .as_object()
            .ok_or_else(|| ActionAdapterError::BadCalldata(format!("{label} must be an object")))
    }

    /// Borrow a JSON object field.
    ///
    /// # Errors
    ///
    /// Returns [`ActionAdapterError::BadCalldata`] when `field` is missing or not an
    /// object.
    pub fn object_field<'a>(
        object: &'a Map<String, Value>,
        field: &str,
    ) -> Result<&'a Map<String, Value>, ActionAdapterError> {
        object
            .get(field)
            .ok_or_else(|| ActionAdapterError::BadCalldata(format!("missing field {field}")))
            .and_then(|value| self::object(value, field))
    }

    /// Borrow a JSON array field.
    ///
    /// # Errors
    ///
    /// Returns [`ActionAdapterError::BadCalldata`] when `field` is missing or not an
    /// array.
    pub fn array_field<'a>(
        object: &'a Map<String, Value>,
        field: &str,
    ) -> Result<&'a [Value], ActionAdapterError> {
        object
            .get(field)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .ok_or_else(|| ActionAdapterError::BadCalldata(format!("{field} must be an array")))
    }

    /// Parse an address field.
    ///
    /// # Errors
    ///
    /// Returns [`ActionAdapterError::BadCalldata`] when the field is missing,
    /// non-stringish, or not an EVM address.
    pub fn address_field(
        object: &Map<String, Value>,
        field: &str,
    ) -> Result<Address, ActionAdapterError> {
        let value = stringish_field(object, field)?;
        Address::new(&value).map_err(ActionAdapterError::BadCalldata)
    }

    /// Parse a u64 field encoded as a JSON string or number.
    ///
    /// # Errors
    ///
    /// Returns [`ActionAdapterError::BadCalldata`] when the field is missing, not a
    /// uint256 decimal, or does not fit in u64.
    pub fn u64_field(object: &Map<String, Value>, field: &str) -> Result<u64, ActionAdapterError> {
        let value = u256_string_field(object, field)?;
        value.parse::<u64>().map_err(|err| {
            ActionAdapterError::BadCalldata(format!("{field} does not fit u64: {err}"))
        })
    }

    /// Parse and normalize a uint256 decimal field.
    ///
    /// # Errors
    ///
    /// Returns [`ActionAdapterError::BadCalldata`] when the field is missing,
    /// non-stringish, or not a uint256 decimal.
    pub fn u256_string_field(
        object: &Map<String, Value>,
        field: &str,
    ) -> Result<String, ActionAdapterError> {
        let value = stringish_field(object, field)?;
        U256::from_str_radix(&value, 10)
            .map(|parsed| parsed.to_string())
            .map_err(|err| {
                ActionAdapterError::BadCalldata(format!("{field} must be uint256: {err}"))
            })
    }

    /// Return a string value from a JSON string or number field.
    ///
    /// # Errors
    ///
    /// Returns [`ActionAdapterError::BadCalldata`] when the field is missing or not
    /// a string/number.
    pub fn stringish_field(
        object: &Map<String, Value>,
        field: &str,
    ) -> Result<String, ActionAdapterError> {
        let value = object
            .get(field)
            .ok_or_else(|| ActionAdapterError::BadCalldata(format!("missing field {field}")))?;
        match value {
            Value::String(s) => Ok(s.clone()),
            Value::Number(n) => Ok(n.to_string()),
            _ => Err(ActionAdapterError::BadCalldata(format!(
                "{field} must be a string or number"
            ))),
        }
    }

    /// Parse a static adapter id and panic if it is malformed.
    #[must_use]
    #[allow(clippy::panic)]
    pub fn static_adapter_id(raw: &str) -> ActionAdapterId {
        ActionAdapterId::new(raw)
            .unwrap_or_else(|err| panic!("invalid static adapter id {raw}: {err}"))
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
    /// Permit2 EIP-712 permit or transfer action family.
    Permit2,
    /// EIP-2612 permit action family.
    Eip2612,
    /// Fallback unknown EIP-712 action family.
    Eip712Other,
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
    pub fn match_key(&self, selector: [u8; 4]) -> TransactionMatchKey {
        TransactionMatchKey::exact(self.chain_id, self.address.clone(), selector)
    }
}

/// Static-ish metadata a registry can index before invoking an adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionActionAdapterDescriptor {
    /// Stable adapter id.
    pub id: ActionAdapterId,
    /// Protocol id used in policy context.
    pub protocol_id: String,
    /// `TransactionActionAdapter` shape.
    pub kind: TransactionActionAdapterKind,
    /// Solidity functions covered by the adapter.
    pub functions: Vec<SolidityFunction>,
    /// Registry match keys covered by the adapter.
    pub match_keys: Vec<TransactionMatchKey>,
    /// Action kinds emitted by the adapter.
    pub emitted_actions: Vec<ActionKind>,
}

impl TransactionActionAdapterDescriptor {
    /// Construct adapter metadata.
    #[must_use]
    pub fn new(
        id: ActionAdapterId,
        protocol_id: &str,
        kind: TransactionActionAdapterKind,
        functions: Vec<SolidityFunction>,
        match_keys: Vec<TransactionMatchKey>,
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
    pub fn from_adapter(adapter: &dyn TransactionActionAdapter) -> Self {
        let id = adapter.id();
        let protocol_id = id.protocol().to_string();
        Self {
            id,
            protocol_id,
            kind: TransactionActionAdapterKind::Function,
            functions: Vec::new(),
            match_keys: adapter.match_keys(),
            emitted_actions: Vec::new(),
        }
    }
}

/// Static-ish metadata a signature registry can index before invoking a
/// signature adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureActionAdapterDescriptor {
    /// Stable adapter id.
    pub id: ActionAdapterId,
    /// Protocol id used in policy context.
    pub protocol_id: String,
    /// Registry match keys covered by the adapter.
    pub match_keys: Vec<SignatureMatchKey>,
    /// Action kinds emitted by the adapter.
    pub emitted_actions: Vec<ActionKind>,
}

impl SignatureActionAdapterDescriptor {
    /// Construct signature adapter metadata.
    #[must_use]
    pub fn new(
        id: ActionAdapterId,
        protocol_id: &str,
        match_keys: Vec<SignatureMatchKey>,
        emitted_actions: Vec<ActionKind>,
    ) -> Self {
        Self {
            id,
            protocol_id: protocol_id.into(),
            match_keys,
            emitted_actions,
        }
    }

    /// Build a minimal descriptor from a runtime signature adapter instance.
    pub fn from_adapter(adapter: &dyn SignatureActionAdapter) -> Self {
        adapter.descriptor()
    }
}

fn construct_declared_transaction_action_adapter<T: DeclaredTransactionActionAdapter>(
) -> Arc<dyn TransactionActionAdapter> {
    Arc::new(T::default())
}

/// Factory surface a remote/local adapter registry can use to instantiate an
/// adapter after it has matched the descriptor.
pub trait TransactionActionAdapterFactory: Send + Sync {
    /// Registry-visible metadata for this factory.
    fn descriptor(&self) -> TransactionActionAdapterDescriptor;
    /// Instantiate the adapter.
    fn create(&self) -> Arc<dyn TransactionActionAdapter>;
}

/// Function pointer used by static adapter factories.
pub type TransactionActionAdapterConstructor = fn() -> Arc<dyn TransactionActionAdapter>;

/// Static factory for adapters that can be constructed with `Default`.
#[derive(Debug, Clone)]
pub struct StaticTransactionActionAdapterFactory {
    descriptor: TransactionActionAdapterDescriptor,
    constructor: TransactionActionAdapterConstructor,
}

impl StaticTransactionActionAdapterFactory {
    /// Construct a factory from metadata and a constructor function.
    pub fn new(
        descriptor: TransactionActionAdapterDescriptor,
        constructor: TransactionActionAdapterConstructor,
    ) -> Self {
        Self {
            descriptor,
            constructor,
        }
    }
}

impl TransactionActionAdapterFactory for StaticTransactionActionAdapterFactory {
    fn descriptor(&self) -> TransactionActionAdapterDescriptor {
        self.descriptor.clone()
    }

    fn create(&self) -> Arc<dyn TransactionActionAdapter> {
        (self.constructor)()
    }
}

/// Declared authoring surface for third-party transaction adapters.
///
/// Implement this trait when one crate/module owns one logical adapter. The
/// associated constants describe the registry-visible surface;
/// `build_transaction_action` is the only protocol-specific runtime function
/// simple adapters must provide.
pub trait DeclaredTransactionActionAdapter: Send + Sync + Default + Sized + 'static {
    /// Stable adapter id.
    const ADAPTER_ID: &'static str;
    /// Protocol id emitted into DEX context.
    const PROTOCOL_ID: &'static str;
    /// `TransactionActionAdapter` shape.
    const KIND: TransactionActionAdapterKind;
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
    fn build_transaction_action(
        &self,
        tx: &TransactionRequest,
    ) -> Result<LegacyAction, ActionAdapterError>;

    /// Parsed static adapter id.
    #[allow(clippy::expect_used)]
    #[must_use]
    fn adapter_id() -> ActionAdapterId {
        ActionAdapterId::new(Self::ADAPTER_ID).expect("static ActionAdapterId is well-formed")
    }

    /// Match keys generated from contract targets and functions.
    fn declared_match_keys(&self) -> Vec<TransactionMatchKey> {
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

    /// Registry descriptor generated from declared adapter constants.
    fn declared_descriptor(&self) -> TransactionActionAdapterDescriptor {
        let id = Self::adapter_id();
        TransactionActionAdapterDescriptor::new(
            id,
            Self::PROTOCOL_ID,
            Self::KIND,
            Self::FUNCTIONS
                .iter()
                .map(|f| f.into_owned_function())
                .collect(),
            self.declared_match_keys(),
            Self::EMITTED_ACTIONS.to_vec(),
        )
    }

    /// Static factory for this declared adapter.
    fn factory() -> StaticTransactionActionAdapterFactory {
        StaticTransactionActionAdapterFactory::new(
            Self::default().declared_descriptor(),
            construct_declared_transaction_action_adapter::<Self>,
        )
    }
}

impl<T> TransactionActionAdapter for T
where
    T: DeclaredTransactionActionAdapter,
{
    fn id(&self) -> ActionAdapterId {
        T::adapter_id()
    }

    fn match_keys(&self) -> Vec<TransactionMatchKey> {
        self.declared_match_keys()
    }

    fn descriptor(&self) -> TransactionActionAdapterDescriptor {
        self.declared_descriptor()
    }

    fn build_action(&self, tx: &TransactionRequest) -> Result<LegacyAction, ActionAdapterError> {
        DeclaredTransactionActionAdapter::build_transaction_action(self, tx)
    }
}

/// One adapter handles one (or a small set of) `(chain_id, to, selector)` keys
/// and emits a `LegacyAction` from a decoded `TransactionRequest`.
pub trait TransactionActionAdapter: Send + Sync {
    /// Stable adapter id.
    fn id(&self) -> ActionAdapterId;

    /// The set of `(chain_id, to, selector)` keys this adapter wants to match.
    /// `to == None` means "any contract address".
    fn match_keys(&self) -> Vec<TransactionMatchKey>;

    /// Registry/catalog metadata. Simple adapters can use the default; richer
    /// adapters should override it with function signatures and action kinds.
    fn descriptor(&self) -> TransactionActionAdapterDescriptor {
        let id = self.id();
        let protocol_id = id.protocol().to_string();
        TransactionActionAdapterDescriptor {
            id,
            protocol_id,
            kind: TransactionActionAdapterKind::Function,
            functions: Vec::new(),
            match_keys: self.match_keys(),
            emitted_actions: Vec::new(),
        }
    }

    /// Try to construct a `LegacyAction` from this transaction. Called only after
    /// the resolver has selected this adapter, so the implementation may
    /// assume the calldata starts with the matching selector.
    /// # Errors
    ///
    /// Returns an error when calldata cannot be decoded or mapped.
    fn build_action(&self, tx: &TransactionRequest) -> Result<LegacyAction, ActionAdapterError>;
}

/// Declared authoring surface for third-party signature adapters.
///
/// Implement this trait when one crate/module owns one logical signature
/// adapter. The associated constants describe the registry-visible surface;
/// `build_signature_action` is the only protocol-specific runtime function
/// simple adapters must provide.
pub trait DeclaredSignatureActionAdapter: Send + Sync + Default + Sized + 'static {
    /// Stable adapter id.
    const ADAPTER_ID: &'static str;
    /// Protocol id emitted into signature context.
    const PROTOCOL_ID: &'static str;
    /// Semantic action families this adapter may emit.
    const EMITTED_ACTIONS: &'static [ActionKind];

    /// The set of `(chain_id, verifying_contract, primary_type)` keys this
    /// adapter wants to match.
    fn match_keys(&self) -> Vec<SignatureMatchKey>;

    /// Build a semantic action from a signature request.
    ///
    /// # Errors
    ///
    /// Returns an error when typed-data decoding or mapping fails.
    fn build_signature_action(
        &self,
        sig: &SignatureRequest,
    ) -> Result<LegacyAction, ActionAdapterError>;

    /// Parsed static adapter id.
    #[allow(clippy::expect_used)]
    #[must_use]
    fn adapter_id() -> ActionAdapterId {
        ActionAdapterId::new(Self::ADAPTER_ID).expect("static ActionAdapterId is well-formed")
    }

    /// Registry descriptor generated from declared adapter constants.
    fn declared_descriptor(&self) -> SignatureActionAdapterDescriptor {
        let id = Self::adapter_id();
        SignatureActionAdapterDescriptor::new(
            id,
            Self::PROTOCOL_ID,
            self.match_keys(),
            Self::EMITTED_ACTIONS.to_vec(),
        )
    }
}

impl<T> SignatureActionAdapter for T
where
    T: DeclaredSignatureActionAdapter,
{
    fn id(&self) -> ActionAdapterId {
        T::adapter_id()
    }

    fn match_keys(&self) -> Vec<SignatureMatchKey> {
        DeclaredSignatureActionAdapter::match_keys(self)
    }

    fn descriptor(&self) -> SignatureActionAdapterDescriptor {
        self.declared_descriptor()
    }

    fn build_action(&self, sig: &SignatureRequest) -> Result<LegacyAction, ActionAdapterError> {
        DeclaredSignatureActionAdapter::build_signature_action(self, sig)
    }
}

/// A single `(chain_id, to, selector)` pattern an adapter matches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransactionMatchKey {
    /// EVM chain id.
    pub chain_id: ChainId,
    /// `None` represents the wildcard (`"*"` in the manifest spec).
    pub to: Option<Address>,
    /// Four-byte function selector.
    pub selector: [u8; 4],
}

impl TransactionMatchKey {
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

    #[derive(Default)]
    struct DeclaredSignatureNoopAdapter;

    impl DeclaredTransactionActionAdapter for TypedNoopAdapter {
        const ADAPTER_ID: &'static str = "test/typed-noop@0.0.1";
        const PROTOCOL_ID: &'static str = "test";
        const KIND: TransactionActionAdapterKind = TransactionActionAdapterKind::Function;
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

        fn build_transaction_action(
            &self,
            tx: &TransactionRequest,
        ) -> Result<LegacyAction, ActionAdapterError> {
            Ok(LegacyAction::Other(OtherAction {
                actor: tx.from.clone(),
                target: tx.to.clone(),
                selector: "0xaabbccdd".into(),
                value_wei: tx.value_wei.clone(),
                raw_calldata: hex::encode(&tx.data),
            }))
        }
    }

    impl DeclaredSignatureActionAdapter for DeclaredSignatureNoopAdapter {
        const ADAPTER_ID: &'static str = "test/signature-noop@0.0.1";
        const PROTOCOL_ID: &'static str = "test-signature";
        const EMITTED_ACTIONS: &'static [ActionKind] = &[ActionKind::Eip712Other];

        fn match_keys(&self) -> Vec<SignatureMatchKey> {
            vec![SignatureMatchKey::exact(
                1,
                Address::new("0x2222222222222222222222222222222222222222").unwrap(),
                "Permit",
            )]
        }

        fn build_signature_action(
            &self,
            sig: &SignatureRequest,
        ) -> Result<LegacyAction, ActionAdapterError> {
            Ok(LegacyAction::Other(OtherAction {
                actor: sig.signer.clone(),
                target: sig.typed_data.domain.verifying_contract.clone(),
                selector: "0x".into(),
                value_wei: "0".into(),
                raw_calldata: "{}".into(),
            }))
        }
    }

    #[test]
    fn typed_adapter_supplies_runtime_adapter_contract() {
        let adapter = TypedNoopAdapter;
        let as_adapter: &dyn TransactionActionAdapter = &adapter;
        let target = Address::new("0x1111111111111111111111111111111111111111").unwrap();

        assert_eq!(
            as_adapter.id(),
            ActionAdapterId::new("test/typed-noop@0.0.1")
                .expect("static ActionAdapterId is well-formed")
        );
        assert_eq!(
            as_adapter.match_keys(),
            vec![TransactionMatchKey::exact(
                1,
                target,
                [0xaa, 0xbb, 0xcc, 0xdd]
            )]
        );

        let descriptor = as_adapter.descriptor();
        assert_eq!(descriptor.protocol_id, "test");
        assert_eq!(descriptor.kind, TransactionActionAdapterKind::Function);
        assert_eq!(descriptor.functions[0].signature, "noop(uint256)");
        assert_eq!(descriptor.emitted_actions, vec![ActionKind::Other]);
    }

    #[test]
    fn typed_adapter_factory_instantiates_adapter() {
        let factory = TypedNoopAdapter::factory();
        assert_eq!(
            factory.descriptor().id,
            ActionAdapterId::new("test/typed-noop@0.0.1")
                .expect("static ActionAdapterId is well-formed")
        );
        assert_eq!(
            factory.create().id(),
            ActionAdapterId::new("test/typed-noop@0.0.1")
                .expect("static ActionAdapterId is well-formed")
        );
    }

    #[test]
    fn declared_signature_adapter_supplies_runtime_adapter_contract() {
        let adapter = DeclaredSignatureNoopAdapter;
        let as_adapter: &dyn SignatureActionAdapter = &adapter;
        let verifying_contract =
            Address::new("0x2222222222222222222222222222222222222222").unwrap();

        assert_eq!(
            as_adapter.id(),
            ActionAdapterId::new("test/signature-noop@0.0.1")
                .expect("static ActionAdapterId is well-formed")
        );
        assert_eq!(
            as_adapter.match_keys(),
            vec![SignatureMatchKey::exact(1, verifying_contract, "Permit")]
        );

        let descriptor = as_adapter.descriptor();
        assert_eq!(descriptor.protocol_id, "test-signature");
        assert_eq!(descriptor.emitted_actions, vec![ActionKind::Eip712Other]);
    }
}
