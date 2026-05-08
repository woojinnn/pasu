//! Core domain types: Address, Token, `AmountSpec`, Action.

use alloy_primitives::{Address as AlloyAddress, U256};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, str::FromStr};
use thiserror::Error;

/// EVM address as a lowercase hex string with 0x prefix.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(String);

impl Address {
    /// Parse and normalize an EVM address.
    ///
    /// # Errors
    ///
    /// Returns an error when `s` is not a valid 20-byte hex address.
    pub fn new(s: &str) -> Result<Self, String> {
        let parsed = AlloyAddress::from_str(s).map_err(|e| format!("invalid address: {e}"))?;
        Ok(Self(format!("{parsed:#x}")))
    }

    /// Convert an Alloy address into the engine address wrapper.
    #[must_use]
    pub fn from_alloy(a: AlloyAddress) -> Self {
        Self(format!("{a:#x}"))
    }

    /// Borrow the normalized lowercase hex string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Engine chain id.
///
/// EIP-712 permits `chainId` to be encoded as a `uint256`, but v1.1 narrows
/// it to `u64`, which covers practical EVM chain ids as of this release. At
/// the Cedar request boundary this value narrows again to Cedar `Long` (`i64`);
/// callers that need to reject oversized ids should do so before building an
/// engine request.
pub type ChainId = u64;

/// Token metadata as the engine sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    /// EVM chain id this token belongs to.
    pub chain_id: ChainId,
    /// Token contract address or native sentinel address.
    pub address: Address,
    /// Human-readable token symbol.
    pub symbol: String,
    /// Token decimal precision.
    pub decimals: u32,
    /// Whether this token represents the native asset.
    pub is_native: bool,
}

impl Token {
    /// Return the chain-qualified token key used by host capability maps.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}:{}", self.chain_id, self.address.as_str().to_lowercase())
    }
}

/// Provenance + value information about an oracle-derived USD valuation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsdValuation {
    /// USD value as a stringified decimal (e.g., "1234.56").
    /// We use a string here because Cedar's `Decimal` extension is the place
    /// that actually does the comparison and we want to avoid f64 drift.
    pub value: String,
    /// Source timestamp for the valuation.
    pub as_of_ts: u64,
    /// Data sources that contributed to the valuation.
    pub sources: Vec<String>,
    /// Age of the valuation in seconds.
    pub stale_sec: u64,
}

/// Amount paired with its token, with optional human-readable and USD views.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmountSpec {
    /// Token this amount is denominated in.
    pub token: Token,
    /// Wei-scale raw amount as decimal string (so we can carry full U256 range).
    pub raw: String,
    /// Optional `raw / 10^decimals` representation as a decimal string.
    pub human: Option<String>,
    /// Optional USD valuation; present when an oracle was available.
    pub usd: Option<UsdValuation>,
}

impl AmountSpec {
    /// Construct an amount from a raw integer value.
    #[must_use]
    pub fn from_raw(token: Token, raw: U256) -> Self {
        Self {
            token,
            raw: raw.to_string(),
            human: None,
            usd: None,
        }
    }
}

/// Which amount side should be valued by the oracle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OracleRequirementKind {
    /// Input amount valuation.
    #[serde(rename = "input")]
    Input,
    /// Minimum output amount valuation.
    #[serde(rename = "minOutput")]
    MinOutput,
}

/// Oracle lookup needed to enrich an action before policy evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OracleRequirement {
    /// Which side of the action this valuation belongs to.
    pub kind: OracleRequirementKind,
    /// Token to value.
    pub token: Token,
    /// Raw token amount to value.
    pub raw_amount: String,
}

/// Projected rolling-window stats stamped into DEX context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WindowStatsContext {
    /// Rolling 24-hour swap volume in USD.
    pub swap_volume_usd_24h: Option<String>,
    /// Rolling 24-hour swap count.
    pub swap_count_24h: Option<u64>,
}

/// Aggregate facts extracted and enriched for a DEX action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DexFacts {
    /// Protocol ids observed in the route.
    pub protocol_ids: Vec<String>,
    /// Tokens spent by the route.
    pub input_tokens: Vec<Token>,
    /// Tokens expected from the route.
    pub output_tokens: Vec<Token>,
    /// Total USD input value, when oracle data is available.
    pub total_input_usd: Option<UsdValuation>,
    /// Total USD minimum output value, when oracle data is available.
    pub total_min_output_usd: Option<UsdValuation>,
    /// Maximum fee across known route legs.
    pub max_fee_bps: Option<u32>,
    /// Whether any route leg has zero minimum output.
    pub has_zero_min_output: bool,
    /// Whether any recipient is external to the actor.
    pub has_external_recipient: bool,
    /// Total input size relative to portfolio, in basis points.
    pub total_input_fraction_of_portfolio_bps: Option<u64>,
    /// Whether allowances cover all non-native inputs.
    pub allowances_cover_inputs: Option<bool>,
    /// Projected stat-window values.
    pub window_stats: Option<WindowStatsContext>,
}

/// Human-readable trace of how an adapter built the DEX action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DexTrace {
    /// Ordered trace steps.
    pub steps: Vec<String>,
}

/// Aggregate DEX action emitted by adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DexAction {
    /// Wallet actor that initiated the transaction.
    pub actor: Address,
    /// Contract target that received the transaction.
    pub target: Address,
    /// Native value attached to the transaction, in wei.
    pub value_wei: String,
    /// Extracted and enriched route facts.
    pub facts: DexFacts,
    /// Oracle lookups needed for enrichment.
    pub oracle_requirements: Vec<OracleRequirement>,
    /// Adapter trace.
    pub trace: DexTrace,
}

/// Fallback action for unrecognized calldata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtherAction {
    /// Wallet actor that initiated the transaction.
    pub actor: Address,
    /// Contract target that received the transaction.
    pub target: Address,
    /// Function selector as hex.
    pub selector: String,
    /// Native value attached to the transaction, in wei.
    pub value_wei: String,
    /// Full calldata as hex.
    pub raw_calldata: String,
}

/// Semantic Permit2 signature action emitted by the Permit2 EIP-712 adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permit2Action {
    /// Wallet that is being asked to sign.
    pub signer: Address,
    /// Chain id supplied by the wallet request.
    pub chain_id: ChainId,
    /// Chain id embedded in the EIP-712 domain.
    pub domain_chain_id: ChainId,
    /// EIP-712 verifying contract.
    pub verifying_contract: Address,
    /// EIP-712 primary type.
    pub primary_type: String,
    /// Permit2 permit shape.
    pub permit_kind: Permit2PermitKind,
    /// Spender authorized by the permit.
    pub spender: Address,
    /// Representative token selected for single-token policy checks.
    pub token: Token,
    /// Representative raw approval amount as a decimal integer string.
    pub amount: String,
    /// Representative approval expiration timestamp.
    pub expiration: u64,
    /// Signature deadline timestamp.
    pub sig_deadline: u64,
    /// Representative nonce as a decimal integer string.
    pub nonce: String,
    /// All approvals decoded from the signature.
    pub approvals: Vec<Permit2Approval>,
    /// Whether any approval carries the Permit2 unlimited uint160 amount.
    pub is_unlimited: bool,
    /// Structural nonce sanity flag.
    pub nonce_valid: bool,
    /// Whether the Permit2 typed data includes a witness payload.
    pub witness_present: bool,
    /// Oracle-derived total approved USD value, when available.
    pub total_approved_usd: Option<UsdValuation>,
}

/// Permit2 permit shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permit2PermitKind {
    /// Permit2 `PermitSingle`.
    #[serde(rename = "PermitSingle")]
    PermitSingle,
    /// Permit2 `PermitBatch`.
    #[serde(rename = "PermitBatch")]
    PermitBatch,
    /// Permit2 `PermitTransferFrom`.
    #[serde(rename = "PermitTransferFrom")]
    PermitTransferFrom,
    /// Permit2 `PermitBatchTransferFrom`.
    #[serde(rename = "PermitBatchTransferFrom")]
    PermitBatchTransferFrom,
    /// Permit2 `PermitWitnessTransferFrom`.
    #[serde(rename = "PermitWitnessTransferFrom")]
    PermitWitnessTransferFrom,
    /// Permit2 `PermitBatchWitnessTransferFrom`.
    #[serde(rename = "PermitBatchWitnessTransferFrom")]
    PermitBatchWitnessTransferFrom,
}

impl Permit2PermitKind {
    /// Return the EIP-712 primary-type label for this Permit2 shape.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PermitSingle => "PermitSingle",
            Self::PermitBatch => "PermitBatch",
            Self::PermitTransferFrom => "PermitTransferFrom",
            Self::PermitBatchTransferFrom => "PermitBatchTransferFrom",
            Self::PermitWitnessTransferFrom => "PermitWitnessTransferFrom",
            Self::PermitBatchWitnessTransferFrom => "PermitBatchWitnessTransferFrom",
        }
    }

    /// Parse a Permit2 primary type label, case-insensitively.
    // `str::eq_ignore_ascii_case` is not const-callable, so this fn cannot be
    // const despite the clippy suggestion.
    #[allow(clippy::missing_const_for_fn)]
    #[must_use]
    pub fn from_primary_type(s: &str) -> Option<Self> {
        if s.eq_ignore_ascii_case(Self::PermitSingle.as_str()) {
            Some(Self::PermitSingle)
        } else if s.eq_ignore_ascii_case(Self::PermitBatch.as_str()) {
            Some(Self::PermitBatch)
        } else if s.eq_ignore_ascii_case(Self::PermitTransferFrom.as_str()) {
            Some(Self::PermitTransferFrom)
        } else if s.eq_ignore_ascii_case(Self::PermitBatchTransferFrom.as_str()) {
            Some(Self::PermitBatchTransferFrom)
        } else if s.eq_ignore_ascii_case(Self::PermitWitnessTransferFrom.as_str()) {
            Some(Self::PermitWitnessTransferFrom)
        } else if s.eq_ignore_ascii_case(Self::PermitBatchWitnessTransferFrom.as_str()) {
            Some(Self::PermitBatchWitnessTransferFrom)
        } else {
            None
        }
    }
}

/// One Permit2 approval item decoded from typed data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permit2Approval {
    /// Token being approved.
    pub token: Token,
    /// Raw approval amount as a decimal integer string.
    pub amount: String,
    /// Approval expiration timestamp.
    pub expiration: u64,
    /// Permit nonce as a decimal integer string.
    pub nonce: String,
}

/// Semantic EIP-2612 permit signature action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eip2612Action {
    /// Address whose permit this is.
    ///
    /// For EIP-2612 this is the permit owner. For ERC-1271/smart-wallet flows
    /// where the ECDSA key differs from the on-chain owner, the host MUST
    /// resolve and pass the owner address as signer before invoking the engine;
    /// the engine does not itself recover signatures.
    pub signer: Address,
    /// Owner carried inside the permit message.
    pub owner: Address,
    /// Chain id supplied by the wallet request.
    pub chain_id: ChainId,
    /// Chain id embedded in the EIP-712 domain.
    pub domain_chain_id: ChainId,
    /// EIP-712 verifying contract.
    pub verifying_contract: Address,
    /// EIP-712 primary type.
    pub primary_type: String,
    /// Spender authorized by the permit.
    pub spender: Address,
    /// Token contract being approved.
    pub token: Token,
    /// Whether the value is uint256 max.
    pub is_unlimited: bool,
    /// Structural nonce sanity flag.
    pub nonce_valid: bool,
    /// Raw approval value as a decimal integer string.
    pub value: String,
    /// Permit deadline timestamp.
    pub deadline: u64,
    /// Permit nonce as a decimal integer string.
    pub nonce: String,
    /// Oracle-derived total approved USD value, when available.
    pub total_approved_usd: Option<UsdValuation>,
}

/// Catch-all action for unmatched EIP-712 signatures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eip712OtherAction {
    /// Wallet that is being asked to sign.
    pub signer: Address,
    /// Chain id supplied by the wallet request.
    pub chain_id: ChainId,
    /// Chain id embedded in the EIP-712 domain.
    pub domain_chain_id: ChainId,
    /// EIP-712 verifying contract.
    pub verifying_contract: Address,
    /// EIP-712 primary type.
    pub primary_type: String,
    /// Optional domain name.
    pub domain_name: Option<String>,
    /// Optional domain version.
    pub domain_version: Option<String>,
    /// Optional domain salt.
    pub domain_salt: Option<String>,
    /// Raw EIP-712 types JSON serialized as compact JSON text.
    pub types_json: String,
    /// Raw EIP-712 message JSON serialized as compact JSON text.
    pub message_json: String,
}

impl Eip712OtherAction {
    /// Construct the catch-all action from an unmatched signature request.
    #[must_use]
    pub fn from_request(sig: &SignatureRequest) -> Self {
        Self {
            signer: sig.signer.clone(),
            chain_id: sig.chain_id,
            domain_chain_id: sig.typed_data.domain.chain_id,
            verifying_contract: sig.typed_data.domain.verifying_contract.clone(),
            primary_type: sig.typed_data.primary_type.clone(),
            domain_name: sig.typed_data.domain.name.clone(),
            domain_version: sig.typed_data.domain.version.clone(),
            domain_salt: sig.typed_data.domain.salt.clone(),
            types_json: json_to_compact_string(&sig.typed_data.types),
            message_json: json_to_compact_string(&sig.typed_data.message),
        }
    }
}

/// Semantic action emitted by adapters.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Aggregate DEX action.
    #[serde(rename = "dex")]
    Dex(DexAction),
    /// Fallback action for unrecognized calls.
    #[serde(rename = "other")]
    Other(OtherAction),
    /// Permit2 EIP-712 signature action.
    #[serde(rename = "permit2")]
    Permit2(Permit2Action),
    /// EIP-2612 Permit EIP-712 signature action.
    #[serde(rename = "eip2612")]
    Eip2612(Eip2612Action),
    /// Catch-all unmatched EIP-712 signature action.
    #[serde(rename = "eip712Other")]
    Eip712Other(Eip712OtherAction),
}

impl Action {
    /// Return the stable action kind string used in Cedar action ids.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Dex(_) => "dex",
            Self::Other(_) => "other",
            Self::Permit2(_) => "signature.permit2",
            Self::Eip2612(_) => "signature.eip2612",
            Self::Eip712Other(_) => "signature.eip712_other",
        }
    }

    /// Return the action target address.
    #[must_use]
    pub const fn target(&self) -> &Address {
        match self {
            Self::Dex(d) => &d.target,
            Self::Other(o) => &o.target,
            Self::Permit2(p) => &p.verifying_contract,
            Self::Eip2612(p) => &p.verifying_contract,
            Self::Eip712Other(o) => &o.verifying_contract,
        }
    }

    /// Return the wallet actor address.
    #[must_use]
    pub const fn actor(&self) -> &Address {
        match self {
            Self::Dex(d) => &d.actor,
            Self::Other(o) => &o.actor,
            Self::Permit2(p) => &p.signer,
            Self::Eip2612(p) => &p.signer,
            Self::Eip712Other(o) => &o.signer,
        }
    }
}

/// Top-level policy-engine request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// EVM transaction request.
    Tx(TransactionRequest),
    /// EIP-712 signature request.
    Sig(SignatureRequest),
}

/// Unsigned transaction request presented to the policy engine.
///
/// Naming aligns with `alloy::TransactionRequest` and
/// `ethers::TransactionRequest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionRequest {
    /// EVM chain id.
    pub chain_id: ChainId,
    /// Sender wallet address.
    pub from: Address,
    /// Target contract address.
    pub to: Address,
    /// `msg.value` in wei, decimal-encoded so we can carry a full U256.
    pub value_wei: String,
    /// Calldata (function selector + ABI-encoded args).
    pub data: Vec<u8>,
    /// Gas limit, when known.
    pub gas: Option<u64>,
    /// Account nonce, when known.
    pub nonce: Option<u64>,
}

/// Off-chain EIP-712 signature request presented to the policy engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureRequest {
    /// EVM chain id selected by the wallet request.
    pub chain_id: ChainId,
    /// Wallet that is being asked to sign.
    pub signer: Address,
    /// Typed data payload.
    pub typed_data: Eip712TypedData,
}

impl SignatureRequest {
    /// Borrow the EIP-712 primary type.
    #[must_use]
    pub fn primary_type(&self) -> &str {
        &self.typed_data.primary_type
    }
}

/// EIP-712 typed-data payload.
// `serde_json::Value` carries an f64 number variant, so `Eq` is intentionally
// not derived — the clippy suggestion to add `Eq` is a false positive here.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip712TypedData {
    /// EIP-712 domain.
    pub domain: Eip712Domain,
    /// EIP-712 primary type.
    pub primary_type: String,
    /// EIP-712 type map.
    pub types: serde_json::Value,
    /// EIP-712 message object.
    pub message: serde_json::Value,
}

/// Error returned when an EIP-712 typed-data payload is structurally invalid.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TypedDataError {
    /// The `types` field was not a JSON object.
    #[error("typedData.types must be a JSON object")]
    TypesNotObject,
    /// The primary type was not present in `types`.
    #[error("typedData.types missing primaryType {primary_type}")]
    MissingPrimaryType {
        /// Missing primary type name.
        primary_type: String,
    },
    /// The primary type entry was not an array.
    #[error("typedData.types[{primary_type}] must be an array")]
    PrimaryTypeNotArray {
        /// Primary type name.
        primary_type: String,
    },
    /// One primary type field entry was not an object.
    #[error("typedData.types[{primary_type}][{index}] must be an object")]
    FieldNotObject {
        /// Primary type name.
        primary_type: String,
        /// Field entry index.
        index: usize,
    },
    /// One primary type field entry had no string `name`.
    #[error("typedData.types[{primary_type}][{index}].name must be a string")]
    FieldNameNotString {
        /// Primary type name.
        primary_type: String,
        /// Field entry index.
        index: usize,
    },
    /// One primary type field entry had no string `type`.
    #[error("typedData.types[{primary_type}][{index}].type must be a string")]
    FieldTypeNotString {
        /// Primary type name.
        primary_type: String,
        /// Field entry index.
        index: usize,
    },
    /// The `message` field was not a JSON object.
    #[error("typedData.message must be a JSON object")]
    MessageNotObject,
    /// The message object did not contain a field declared by the primary type.
    #[error("typedData.message missing primaryType field {field}")]
    MissingMessageField {
        /// Missing message field.
        field: String,
    },
    /// The `types` map did not declare the EIP-712 domain type.
    #[error("MissingEip712Domain: typedData.types missing EIP712Domain")]
    MissingEip712Domain,
    /// A declared field type was not a valid Solidity EIP-712 type string.
    #[error(
        "InvalidType: typedData.types[{primary_type}].{field_name} has invalid type {type_string}"
    )]
    InvalidType {
        /// Type containing the invalid field declaration.
        primary_type: String,
        /// Field whose type was invalid.
        field_name: String,
        /// Invalid type string.
        type_string: String,
    },
    /// A custom type reference pointed to a type not declared in `types`.
    #[error("MissingReferencedType: {referenced_from} references missing type {missing_type}")]
    MissingReferencedType {
        /// Type containing the missing reference.
        referenced_from: String,
        /// Missing referenced type name.
        missing_type: String,
    },
    /// The custom type graph contains a cycle.
    #[error("TypeCycle: typedData type graph contains a cycle at {type_name}")]
    TypeCycle {
        /// Type detected on the active DFS stack.
        type_name: String,
    },
}

/// Validate the EIP-712 typed-data shape needed before adapter dispatch.
///
/// This checks the declared `EIP712Domain`, validates reachable field type
/// strings, rejects missing reachable custom types and cycles, and preserves the
/// top-level primary type contract that `message` must contain each declared
/// primary-type field.
///
/// # Errors
///
/// Returns [`TypedDataError`] when the payload is structurally invalid.
pub fn validate_typed_data(td: &Eip712TypedData) -> Result<(), TypedDataError> {
    let primary_type = td.primary_type.as_str();
    let types = td.types.as_object().ok_or(TypedDataError::TypesNotObject)?;

    if !types.contains_key(EIP712_DOMAIN_TYPE) {
        return Err(TypedDataError::MissingEip712Domain);
    }

    let mut visit_states = HashMap::new();
    walk_typed_data_type(
        EIP712_DOMAIN_TYPE,
        None,
        primary_type,
        types,
        &mut visit_states,
    )?;
    walk_typed_data_type(primary_type, None, primary_type, types, &mut visit_states)?;

    let entries = typed_data_type_entries(types, primary_type, None, primary_type)?;
    let message = td
        .message
        .as_object()
        .ok_or(TypedDataError::MessageNotObject)?;

    for (index, entry) in entries.iter().enumerate() {
        let entry = entry
            .as_object()
            .ok_or_else(|| TypedDataError::FieldNotObject {
                primary_type: primary_type.into(),
                index,
            })?;
        let name = entry.get("name").and_then(Value::as_str).ok_or_else(|| {
            TypedDataError::FieldNameNotString {
                primary_type: primary_type.into(),
                index,
            }
        })?;
        entry.get("type").and_then(Value::as_str).ok_or_else(|| {
            TypedDataError::FieldTypeNotString {
                primary_type: primary_type.into(),
                index,
            }
        })?;
        if !message.contains_key(name) {
            return Err(TypedDataError::MissingMessageField { field: name.into() });
        }
    }

    Ok(())
}

const EIP712_DOMAIN_TYPE: &str = "EIP712Domain";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Visited,
}

fn walk_typed_data_type(
    type_name: &str,
    referenced_from: Option<&str>,
    primary_type: &str,
    types: &serde_json::Map<String, Value>,
    visit_states: &mut HashMap<String, VisitState>,
) -> Result<(), TypedDataError> {
    match visit_states.get(type_name) {
        Some(VisitState::Visiting) => {
            return Err(TypedDataError::TypeCycle {
                type_name: type_name.into(),
            });
        }
        Some(VisitState::Visited) => return Ok(()),
        None => {}
    }

    let entries = typed_data_type_entries(types, type_name, referenced_from, primary_type)?;
    visit_states.insert(type_name.into(), VisitState::Visiting);

    for referenced_type in typed_data_custom_references(type_name, entries)? {
        walk_typed_data_type(
            &referenced_type,
            Some(type_name),
            primary_type,
            types,
            visit_states,
        )?;
    }

    visit_states.insert(type_name.into(), VisitState::Visited);
    Ok(())
}

fn typed_data_type_entries<'a>(
    types: &'a serde_json::Map<String, Value>,
    type_name: &str,
    referenced_from: Option<&str>,
    primary_type: &str,
) -> Result<&'a Vec<Value>, TypedDataError> {
    let Some(value) = types.get(type_name) else {
        if type_name == primary_type {
            return Err(TypedDataError::MissingPrimaryType {
                primary_type: primary_type.into(),
            });
        }
        return Err(TypedDataError::MissingReferencedType {
            referenced_from: referenced_from.unwrap_or(primary_type).into(),
            missing_type: type_name.into(),
        });
    };
    value
        .as_array()
        .ok_or_else(|| TypedDataError::PrimaryTypeNotArray {
            primary_type: type_name.into(),
        })
}

fn typed_data_custom_references(
    type_name: &str,
    entries: &[Value],
) -> Result<Vec<String>, TypedDataError> {
    let mut references = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let entry = entry
            .as_object()
            .ok_or_else(|| TypedDataError::FieldNotObject {
                primary_type: type_name.into(),
                index,
            })?;
        let field_name = entry.get("name").and_then(Value::as_str).ok_or_else(|| {
            TypedDataError::FieldNameNotString {
                primary_type: type_name.into(),
                index,
            }
        })?;
        let type_string = entry.get("type").and_then(Value::as_str).ok_or_else(|| {
            TypedDataError::FieldTypeNotString {
                primary_type: type_name.into(),
                index,
            }
        })?;

        match parse_solidity_type(type_string) {
            Ok(Some(referenced_type)) => references.push(referenced_type.into()),
            Ok(None) => {}
            Err(()) => {
                return Err(TypedDataError::InvalidType {
                    primary_type: type_name.into(),
                    field_name: field_name.into(),
                    type_string: type_string.into(),
                });
            }
        }
    }
    Ok(references)
}

fn parse_solidity_type(type_string: &str) -> Result<Option<&str>, ()> {
    let base = strip_array_suffixes(type_string)?;
    if is_primitive_solidity_type(base) {
        return Ok(None);
    }
    if is_invalid_primitive_like_type(base) {
        return Err(());
    }
    if is_custom_type_name(base) {
        return Ok(Some(base));
    }
    Err(())
}

fn strip_array_suffixes(mut type_string: &str) -> Result<&str, ()> {
    if type_string.is_empty() {
        return Err(());
    }

    while type_string.ends_with(']') {
        let Some(open_index) = type_string.rfind('[') else {
            return Err(());
        };
        let length = &type_string[(open_index + 1)..(type_string.len() - 1)];
        if !length.is_empty() && !length.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(());
        }
        type_string = &type_string[..open_index];
        if type_string.is_empty() {
            return Err(());
        }
    }

    if type_string.contains('[') || type_string.contains(']') {
        return Err(());
    }

    Ok(type_string)
}

fn is_primitive_solidity_type(base: &str) -> bool {
    matches!(base, "address" | "bool" | "string" | "bytes")
        || fixed_bytes_width(base).is_some_and(|width| (1..=32).contains(&width))
        || integer_width(base, "uint").is_some_and(valid_integer_width)
        || integer_width(base, "int").is_some_and(valid_integer_width)
}

fn is_invalid_primitive_like_type(base: &str) -> bool {
    if matches!(base, "uint" | "int") {
        return true;
    }
    integer_width(base, "uint").is_some_and(|width| !valid_integer_width(width))
        || integer_width(base, "int").is_some_and(|width| !valid_integer_width(width))
        || fixed_bytes_width(base).is_some_and(|width| !(1..=32).contains(&width))
}

fn integer_width(base: &str, prefix: &str) -> Option<u16> {
    let suffix = base.strip_prefix(prefix)?;
    if suffix.is_empty() || !suffix.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}

fn fixed_bytes_width(base: &str) -> Option<u16> {
    let suffix = base.strip_prefix("bytes")?;
    if suffix.is_empty() || !suffix.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}

fn valid_integer_width(width: u16) -> bool {
    (8..=256).contains(&width) && width.is_multiple_of(8)
}

fn is_custom_type_name(base: &str) -> bool {
    let mut chars = base.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// EIP-712 domain fields used by v1 signature policies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eip712Domain {
    /// Optional domain name.
    pub name: Option<String>,
    /// Optional domain version.
    pub version: Option<String>,
    /// EIP-712 domain chain id.
    pub chain_id: ChainId,
    /// EIP-712 verifying contract.
    pub verifying_contract: Address,
    /// Optional domain salt.
    pub salt: Option<String>,
}

fn json_to_compact_string(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
}

impl TransactionRequest {
    /// First four bytes of `data`, if present, as the ABI function selector.
    #[must_use]
    pub fn selector(&self) -> Option<[u8; 4]> {
        if self.data.len() < 4 {
            return None;
        }
        Some([self.data[0], self.data[1], self.data[2], self.data[3]])
    }

    /// First four bytes of `data`, if present, as `0x`-prefixed hex.
    #[must_use]
    pub fn selector_hex(&self) -> Option<String> {
        self.selector().map(|s| format!("0x{}", hex::encode(s)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_normalizes_to_lowercase_hex() {
        let a = Address::new("0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap();
        assert_eq!(a.as_str(), "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48");
    }

    #[test]
    fn address_rejects_garbage() {
        assert!(Address::new("not-an-address").is_err());
        assert!(Address::new("0x1234").is_err());
    }

    #[test]
    fn token_key_is_chain_qualified() {
        let t = Token {
            chain_id: 1,
            address: Address::new("0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap(),
            symbol: "USDC".into(),
            decimals: 6,
            is_native: false,
        };
        assert_eq!(t.key(), "1:0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48");
    }

    #[test]
    fn tx_selector_is_first_four_bytes() {
        let mut data = vec![0x41, 0x4b, 0xf3, 0x89];
        data.extend_from_slice(&[0u8; 32]);
        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new("0x0000000000000000000000000000000000000002").unwrap(),
            value_wei: "0".into(),
            data,
            gas: None,
            nonce: None,
        };
        assert_eq!(tx.selector_hex().unwrap(), "0x414bf389");
    }

    #[test]
    fn tx_selector_returns_none_for_short_data() {
        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new("0x0000000000000000000000000000000000000002").unwrap(),
            value_wei: "0".into(),
            data: vec![0x41, 0x4b],
            gas: None,
            nonce: None,
        };
        assert!(tx.selector_hex().is_none());
    }

    #[test]
    fn tx_carries_optional_gas_and_nonce() {
        let tx = TransactionRequest {
            chain_id: 1,
            from: Address::new("0x0000000000000000000000000000000000000001").unwrap(),
            to: Address::new("0x0000000000000000000000000000000000000002").unwrap(),
            value_wei: "0".into(),
            data: vec![0x00, 0x01, 0x02, 0x03],
            gas: Some(200_000),
            nonce: Some(42),
        };
        assert_eq!(tx.gas, Some(200_000));
        assert_eq!(tx.nonce, Some(42));
    }

    #[test]
    fn dex_action_kind_actor_and_target_are_transaction_level() {
        let actor = Address::new("0x0000000000000000000000000000000000000001").unwrap();
        let target = Address::new("0x0000000000000000000000000000000000000002").unwrap();
        let action = Action::Dex(DexAction {
            actor: actor.clone(),
            target: target.clone(),
            value_wei: "0".into(),
            facts: DexFacts::default(),
            oracle_requirements: Vec::new(),
            trace: DexTrace::default(),
        });

        assert_eq!(action.kind(), "dex");
        assert_eq!(action.actor(), &actor);
        assert_eq!(action.target(), &target);
    }

    #[test]
    fn other_action_kind_actor_and_target_are_transaction_level() {
        let actor = Address::new("0x0000000000000000000000000000000000000001").unwrap();
        let target = Address::new("0x0000000000000000000000000000000000000002").unwrap();
        let action = Action::Other(OtherAction {
            actor: actor.clone(),
            target: target.clone(),
            selector: "0x12345678".into(),
            value_wei: "7".into(),
            raw_calldata: "0x12345678".into(),
        });

        assert_eq!(action.kind(), "other");
        assert_eq!(action.actor(), &actor);
        assert_eq!(action.target(), &target);
    }

    #[test]
    fn action_serializes_with_kind_variant_names() {
        let actor = Address::new("0x0000000000000000000000000000000000000001").unwrap();
        let target = Address::new("0x0000000000000000000000000000000000000002").unwrap();

        let dex = Action::Dex(DexAction {
            actor: actor.clone(),
            target: target.clone(),
            value_wei: "0".into(),
            facts: DexFacts::default(),
            oracle_requirements: Vec::new(),
            trace: DexTrace::default(),
        });
        let other = Action::Other(OtherAction {
            actor,
            target,
            selector: "0x12345678".into(),
            value_wei: "7".into(),
            raw_calldata: "0x12345678".into(),
        });

        assert!(serde_json::to_value(dex).unwrap().get("dex").is_some());
        assert!(serde_json::to_value(other).unwrap().get("other").is_some());
    }

    #[test]
    fn action_dex_kind_string() {
        let zero = Address::new("0x0000000000000000000000000000000000000000").unwrap();
        let act = Action::Dex(DexAction {
            actor: zero.clone(),
            target: zero,
            value_wei: "0".into(),
            facts: DexFacts::default(),
            oracle_requirements: Vec::new(),
            trace: DexTrace::default(),
        });
        assert_eq!(act.kind(), "dex");
    }
}
