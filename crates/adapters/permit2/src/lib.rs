//! Permit2 EIP-712 signature adapter.

#![deny(unsafe_code)]
#![deny(unused_must_use)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]
#![warn(rust_2018_idioms)]
#![warn(rust_2021_compatibility)]
#![warn(missing_debug_implementations)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::dbg_macro)]
#![warn(clippy::todo)]
#![cfg_attr(not(test), warn(clippy::expect_used))]
#![cfg_attr(not(test), warn(clippy::panic))]
#![cfg_attr(not(test), warn(clippy::unwrap_used))]

use alloy_primitives::U512;
use policy_engine::adapter::signature_helpers::{
    address_field, array_field, object, object_field, panic_static, static_adapter_id,
    static_token, u256_string_field, u64_field, TokenLookup,
};
use policy_engine::lowering::decimal::{
    CEDAR_DECIMAL_CEILING_FRACTION, HUMAN_DECIMAL_SCALE, HUMAN_INT_CEILING,
};
use policy_engine::prelude::*;
use serde_json::{Map, Value};
use std::cmp::Ordering;

/// Permit2 canonical deployment address.
pub const PERMIT2_ADDRESS: &str = "0x000000000022d473030f116ddee9f6b43ac78ba3";

const UINT160_MAX_DEC: &str = "1461501637330902918203684832716283019655932542975";
const UINT256_MAX_DEC: &str =
    "115792089237316195423570985008687907853269984665640564039457584007913129639935";

/// Permit2 EIP-712 adapter.
#[derive(Debug, Clone)]
pub struct Permit2Adapter {
    chain_ids: Vec<ChainId>,
    tokens: TokenLookup,
}

impl Permit2Adapter {
    /// Construct an adapter for mainnet and common L2 Permit2 deployments.
    #[must_use]
    pub fn new() -> Self {
        Self {
            chain_ids: vec![1, 137],
            tokens: default_token_lookup(),
        }
    }

    /// Returns this adapter after adding `token` to its lookup.
    #[must_use]
    pub fn with_token(mut self, token: Token) -> Self {
        self.tokens.add(token);
        self
    }
}

impl Default for Permit2Adapter {
    fn default() -> Self {
        Self::new()
    }
}

impl SignatureAdapter for Permit2Adapter {
    fn id(&self) -> AdapterId {
        static_adapter_id("permit2/eip712@0.1.0")
    }

    fn match_keys(&self) -> Vec<SignatureMatchKey> {
        let verifying_contract = permit2_address();
        self.chain_ids
            .iter()
            .flat_map(|chain_id| {
                [
                    Permit2PermitKind::PermitSingle,
                    Permit2PermitKind::PermitBatch,
                    Permit2PermitKind::PermitTransferFrom,
                    Permit2PermitKind::PermitBatchTransferFrom,
                    Permit2PermitKind::PermitWitnessTransferFrom,
                    Permit2PermitKind::PermitBatchWitnessTransferFrom,
                ]
                .into_iter()
                .map({
                    let verifying_contract = verifying_contract.clone();
                    move |permit_kind| {
                        SignatureMatchKey::exact(
                            *chain_id,
                            verifying_contract.clone(),
                            permit_kind.as_str(),
                        )
                    }
                })
            })
            .collect()
    }

    fn build(&self, sig: &SignatureRequest) -> Result<Action, AdapterError> {
        let Some(permit_kind) = Permit2PermitKind::from_primary_type(sig.primary_type()) else {
            return Err(AdapterError::BadCalldata(format!(
                "unsupported Permit2 primaryType {}",
                sig.primary_type()
            )));
        };
        let decoded = match permit_kind {
            Permit2PermitKind::PermitSingle => self.decode_single(sig)?,
            Permit2PermitKind::PermitBatch => self.decode_batch(sig)?,
            Permit2PermitKind::PermitTransferFrom => self.decode_transfer(sig)?,
            Permit2PermitKind::PermitBatchTransferFrom => self.decode_batch_transfer(sig)?,
            Permit2PermitKind::PermitWitnessTransferFrom => self.decode_witness_transfer(sig)?,
            Permit2PermitKind::PermitBatchWitnessTransferFrom => {
                self.decode_batch_witness_transfer(sig)?
            }
        };
        Ok(Action::Permit2(decoded))
    }
}

impl Permit2Adapter {
    fn decode_single(&self, sig: &SignatureRequest) -> Result<Permit2Action, AdapterError> {
        let message = object(&sig.typed_data.message, "message")?;
        let details = object_field(message, "details")?;
        let approval = self.approval_from_details(sig.chain_id, details)?;
        let spender = address_field(message, "spender")?;
        let sig_deadline = u64_field(message, "sigDeadline")?;
        self.action_from_parts(
            sig,
            Permit2PermitKind::PermitSingle,
            spender,
            sig_deadline,
            vec![approval],
            false,
        )
    }

    fn decode_batch(&self, sig: &SignatureRequest) -> Result<Permit2Action, AdapterError> {
        let message = object(&sig.typed_data.message, "message")?;
        let details = array_field(message, "details")?;
        let approvals = details
            .iter()
            .map(|value| {
                object(value, "details[]")
                    .and_then(|item| self.approval_from_details(sig.chain_id, item))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let spender = address_field(message, "spender")?;
        let sig_deadline = u64_field(message, "sigDeadline")?;
        self.action_from_parts(
            sig,
            Permit2PermitKind::PermitBatch,
            spender,
            sig_deadline,
            approvals,
            false,
        )
    }

    fn decode_transfer(&self, sig: &SignatureRequest) -> Result<Permit2Action, AdapterError> {
        self.decode_transfer_kind(sig, Permit2PermitKind::PermitTransferFrom, false)
    }

    fn decode_batch_transfer(&self, sig: &SignatureRequest) -> Result<Permit2Action, AdapterError> {
        self.decode_batch_transfer_kind(sig, Permit2PermitKind::PermitBatchTransferFrom, false)
    }

    fn decode_witness_transfer(
        &self,
        sig: &SignatureRequest,
    ) -> Result<Permit2Action, AdapterError> {
        self.decode_transfer_kind(sig, Permit2PermitKind::PermitWitnessTransferFrom, true)
    }

    fn decode_batch_witness_transfer(
        &self,
        sig: &SignatureRequest,
    ) -> Result<Permit2Action, AdapterError> {
        self.decode_batch_transfer_kind(
            sig,
            Permit2PermitKind::PermitBatchWitnessTransferFrom,
            true,
        )
    }

    fn decode_transfer_kind(
        &self,
        sig: &SignatureRequest,
        permit_kind: Permit2PermitKind,
        require_witness: bool,
    ) -> Result<Permit2Action, AdapterError> {
        let message = object(&sig.typed_data.message, "message")?;
        let witness_present = self.witness_present(message, require_witness)?;
        let permitted = object_field(message, "permitted")?;
        let token = self
            .tokens
            .get(sig.chain_id, &address_field(permitted, "token")?);
        let amount = u256_string_field(permitted, "amount")?;
        let nonce = u256_string_field(message, "nonce")?;
        let deadline = u64_field(message, "deadline")?;
        let spender = address_field(message, "spender")?;
        let approval = Permit2Approval {
            token,
            amount,
            // TransferFrom shapes do not carry per-permit expirations; only the
            // signature deadline is available at the top level.
            expiration: 0,
            nonce,
        };
        self.action_from_parts(
            sig,
            permit_kind,
            spender,
            deadline,
            vec![approval],
            witness_present,
        )
    }

    fn decode_batch_transfer_kind(
        &self,
        sig: &SignatureRequest,
        permit_kind: Permit2PermitKind,
        require_witness: bool,
    ) -> Result<Permit2Action, AdapterError> {
        let message = object(&sig.typed_data.message, "message")?;
        let witness_present = self.witness_present(message, require_witness)?;
        let permitted = array_field(message, "permitted")?;
        let nonce = u256_string_field(message, "nonce")?;
        let approvals = permitted
            .iter()
            .map(|value| {
                object(value, "permitted[]").and_then(|item| {
                    let token = self
                        .tokens
                        .get(sig.chain_id, &address_field(item, "token")?);
                    Ok(Permit2Approval {
                        token,
                        amount: u256_string_field(item, "amount")?,
                        // TransferFrom shapes do not carry per-permit
                        // expirations; only the signature deadline is available
                        // at the top level.
                        expiration: 0,
                        nonce: nonce.clone(),
                    })
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let deadline = u64_field(message, "deadline")?;
        let spender = address_field(message, "spender")?;
        self.action_from_parts(
            sig,
            permit_kind,
            spender,
            deadline,
            approvals,
            witness_present,
        )
    }

    #[allow(clippy::unused_self)]
    fn witness_present(
        &self,
        message: &Map<String, Value>,
        require_witness: bool,
    ) -> Result<bool, AdapterError> {
        let witness_present = message.contains_key("witness");
        if require_witness && !witness_present {
            return Err(AdapterError::BadCalldata("missing field witness".into()));
        }
        Ok(witness_present)
    }

    fn approval_from_details(
        &self,
        chain_id: ChainId,
        details: &Map<String, Value>,
    ) -> Result<Permit2Approval, AdapterError> {
        let token = self.tokens.get(chain_id, &address_field(details, "token")?);
        Ok(Permit2Approval {
            token,
            amount: u256_string_field(details, "amount")?,
            expiration: u64_field(details, "expiration")?,
            nonce: u256_string_field(details, "nonce")?,
        })
    }

    #[allow(clippy::unused_self)]
    fn action_from_parts(
        &self,
        sig: &SignatureRequest,
        permit_kind: Permit2PermitKind,
        spender: Address,
        sig_deadline: u64,
        approvals: Vec<Permit2Approval>,
        witness_present: bool,
    ) -> Result<Permit2Action, AdapterError> {
        let representative = representative_approval(&approvals)?;
        let is_unlimited = approvals
            .iter()
            .any(|approval| approval.amount == UINT160_MAX_DEC);
        let nonce_valid = approvals
            .iter()
            .all(|approval| approval.nonce != UINT256_MAX_DEC);

        Ok(Permit2Action {
            signer: sig.signer.clone(),
            chain_id: sig.chain_id,
            domain_chain_id: sig.typed_data.domain.chain_id,
            verifying_contract: sig.typed_data.domain.verifying_contract.clone(),
            primary_type: sig.typed_data.primary_type.clone(),
            permit_kind,
            spender,
            token: representative.token.clone(),
            amount: representative.amount.clone(),
            expiration: representative.expiration,
            sig_deadline,
            nonce: representative.nonce.clone(),
            approvals,
            is_unlimited,
            nonce_valid,
            witness_present,
            total_approved_usd: None,
        })
    }
}

fn default_token_lookup() -> TokenLookup {
    TokenLookup::with_tokens([
        static_token(1, "0xdac17f958d2ee523a2206206994597c13d831ec7", "USDT", 6),
        static_token(1, "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6),
        static_token(1, "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "WETH", 18),
        static_token(137, "0x2791bca1f2de4661ed88a30c99a7a9449aa84174", "USDC", 6),
        static_token(137, "0xc2132d05d31c914a87c6611c10748aeb04b58e8f", "USDT", 6),
        static_token(
            137,
            "0x7ceb23fd6bc0add59e62ac25578270cff1b9f619",
            "WETH",
            18,
        ),
    ])
}

fn permit2_address() -> Address {
    Address::new(PERMIT2_ADDRESS)
        .unwrap_or_else(|err| panic_static(&format!("invalid Permit2 address: {err}")))
}

fn representative_approval(
    approvals: &[Permit2Approval],
) -> Result<&Permit2Approval, AdapterError> {
    approvals
        .iter()
        .max_by(|left, right| cmp_approval_human_amount(left, right))
        .ok_or_else(|| AdapterError::BadCalldata("Permit2 approval list is empty".into()))
}

#[derive(Debug, Clone, Copy)]
struct HumanAmountKey {
    raw: U512,
    scale: U512,
    clamped: bool,
}

fn cmp_approval_human_amount(left: &Permit2Approval, right: &Permit2Approval) -> Ordering {
    let left = human_amount_key(left);
    let right = human_amount_key(right);

    match (left.clamped, right.clamped) {
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }

    let left_human = left.raw.saturating_mul(right.scale);
    let right_human = right.raw.saturating_mul(left.scale);
    left_human
        .cmp(&right_human)
        .then_with(|| left.raw.cmp(&right.raw))
}

fn human_amount_key(approval: &Permit2Approval) -> HumanAmountKey {
    let raw = U512::from_str_radix(&approval.amount, 10).unwrap_or(U512::ZERO);
    let scale = decimal_scale(approval.token.decimals);
    HumanAmountKey {
        raw,
        scale,
        clamped: human_decimal_clamps(raw, scale),
    }
}

fn human_decimal_clamps(raw: U512, scale: U512) -> bool {
    let integer_part = raw / scale;
    let ceiling_integer = U512::from(HUMAN_INT_CEILING);
    if integer_part > ceiling_integer {
        return true;
    }

    let fractional_raw = raw % scale;
    let fractional_part = fractional_raw.saturating_mul(U512::from(HUMAN_DECIMAL_SCALE)) / scale;

    integer_part == ceiling_integer && fractional_part > U512::from(CEDAR_DECIMAL_CEILING_FRACTION)
}

fn decimal_scale(decimals: u32) -> U512 {
    let mut scale = U512::from(1u64);
    for _ in 0..decimals {
        scale = scale.saturating_mul(U512::from(10u64));
    }
    scale
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token_fixture(chain_id: ChainId, address: &str, symbol: &str, decimals: u32) -> Token {
        Token {
            chain_id,
            address: Address::new(address).unwrap(),
            symbol: symbol.into(),
            decimals,
            is_native: false,
        }
    }

    fn approval(token: Token, amount: &str) -> Permit2Approval {
        Permit2Approval {
            token,
            amount: amount.into(),
            expiration: 4600,
            nonce: "1".into(),
        }
    }

    #[test]
    fn default_tokens_resolve_polygon_usdc() {
        let lookup = default_token_lookup();
        let token = lookup.get(
            137,
            &Address::new("0x2791bca1f2de4661ed88a30c99a7a9449aa84174").unwrap(),
        );

        assert_eq!(token.decimals, 6);
        assert_ne!(token.symbol, "UNKNOWN");
    }

    #[test]
    fn representative_approval_uses_largest_human_amount() {
        let weth = token_fixture(1, "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "WETH", 18);
        let usdc = token_fixture(1, "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6);
        let approvals = vec![
            approval(weth, "10000000000000000"),
            approval(usdc, "50000000"),
        ];

        let representative = representative_approval(&approvals).unwrap();

        assert_eq!(representative.token.symbol, "USDC");
        assert_eq!(representative.amount, "50000000");
    }

    #[test]
    fn representative_approval_prefers_human_decimal_clamp() {
        let weth = token_fixture(1, "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", "WETH", 18);
        let usdc = token_fixture(1, "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", "USDC", 6);
        let approvals = vec![
            approval(weth, "1000000000000000000000000000000"),
            approval(usdc, "1000000000000000000000000"),
        ];

        let representative = representative_approval(&approvals).unwrap();

        assert_eq!(representative.token.symbol, "USDC");
    }
}
