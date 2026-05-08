//! Core domain types: Address, Token, `AmountSpec`, Action.

use alloy_primitives::{Address as AlloyAddress, U256};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

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

/// Chain id (EIP-155).
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
    /// Oracle-derived total approved USD value, when available.
    pub total_approved_usd: Option<UsdValuation>,
}

/// Permit2 permit shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permit2PermitKind {
    /// Permit2 `PermitSingle`.
    PermitSingle,
    /// Permit2 `PermitBatch`.
    PermitBatch,
    /// Permit2 `PermitTransferFrom`.
    PermitTransferFrom,
}

impl Permit2PermitKind {
    /// Return the EIP-712 primary-type label for this Permit2 shape.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PermitSingle => "PermitSingle",
            Self::PermitBatch => "PermitBatch",
            Self::PermitTransferFrom => "PermitTransferFrom",
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
    /// Wallet that is being asked to sign.
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
    /// Domain name, or an empty string if absent.
    pub domain_name: String,
    /// Domain version, or an empty string if absent.
    pub domain_version: String,
    /// Domain salt, or an empty string if absent.
    pub domain_salt: String,
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
            domain_name: sig.typed_data.domain.name.clone().unwrap_or_default(),
            domain_version: sig.typed_data.domain.version.clone().unwrap_or_default(),
            domain_salt: sig.typed_data.domain.salt.clone().unwrap_or_default(),
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
    match serde_json::to_string(value) {
        Ok(raw) => raw,
        Err(_) => "null".into(),
    }
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
