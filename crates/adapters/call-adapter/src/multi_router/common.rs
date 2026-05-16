//! Shared helpers for the Universal Router multi-call adapter.
//!
//! Word-level ABI readers, asset/recipient utilities, V3 packed-path parsing,
//! and decimal conversion. Used by the per-opcode decoders in
//! `super::command_decode` and `super::v4_actions`.

use std::str::FromStr as _;

use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef, Category,
    DecimalString,
};
use policy_engine::action::dex::SwapAction;

use crate::{AdapterError, CallContext};

pub(super) const WORD_LEN: usize = 32;
pub(super) const ADDRESS_LEN: usize = 20;

pub(super) const WETH_MAINNET: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
pub(super) const ACTION_MSG_SENDER: &str = "0x0000000000000000000000000000000000000001";
pub(super) const ACTION_ADDRESS_THIS: &str = "0x0000000000000000000000000000000000000002";

pub(super) fn swap_envelope(action: SwapAction) -> ActionEnvelope {
    ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(action),
    }
}

pub(super) fn amount_constraint(kind: AmountKind, value: DecimalString) -> AmountConstraint {
    AmountConstraint {
        kind,
        value: Some(value),
    }
}

pub(super) fn asset_ref(ctx: &CallContext<'_>, address: &Address) -> AssetRef {
    let metadata = ctx.token_registry.lookup(ctx.chain_id, address);
    AssetRef {
        kind: AssetKind::Erc20,
        address: Some(address.clone()),
        token_id: None,
        symbol: metadata.as_ref().map(|m| m.symbol.clone()),
        decimals: metadata.map(|m| m.decimals),
    }
}

pub(super) fn native_asset(_ctx: &CallContext<'_>) -> AssetRef {
    AssetRef {
        kind: AssetKind::Native,
        address: None,
        token_id: None,
        symbol: Some("ETH".to_owned()),
        decimals: Some(18),
    }
}

pub(super) fn weth_asset(ctx: &CallContext<'_>) -> AssetRef {
    let weth_addr = Address::from_str(WETH_MAINNET).expect("static WETH address valid");
    let metadata = ctx.token_registry.lookup(ctx.chain_id, &weth_addr);
    AssetRef {
        kind: AssetKind::Erc20,
        address: Some(weth_addr),
        token_id: None,
        symbol: metadata
            .as_ref()
            .map(|m| m.symbol.clone())
            .or_else(|| Some("WETH".to_owned())),
        decimals: metadata.map(|m| m.decimals).or(Some(18)),
    }
}

/// V4 represents native ETH as `address(0)`. Map that to a `Native` AssetRef;
/// any other address is treated as ERC-20.
pub(super) fn v4_asset_ref(ctx: &CallContext<'_>, address: &Address) -> AssetRef {
    let lower = address.to_string().to_ascii_lowercase();
    if lower == "0x0000000000000000000000000000000000000000" {
        return native_asset(ctx);
    }
    asset_ref(ctx, address)
}

/// New helper for the post-PR-12 schema: every action that takes a
/// `(token, amount)` pair now uses the composite `AssetRefWithAmountConstraint`.
pub(super) fn asset_with_amount(
    asset: AssetRef,
    amount: AmountConstraint,
) -> policy_engine::action::AssetRefWithAmountConstraint {
    policy_engine::action::AssetRefWithAmountConstraint { asset, amount }
}

pub(super) fn map_recipient(ctx: &CallContext<'_>, recipient: Address) -> Address {
    let recipient_text = recipient.to_string();
    if recipient_text == ACTION_MSG_SENDER {
        ctx.from.clone()
    } else if recipient_text == ACTION_ADDRESS_THIS {
        ctx.to.clone()
    } else {
        recipient
    }
}

pub(super) struct ParsedV3Path {
    pub(super) token_in: Address,
    pub(super) token_out: Address,
    pub(super) fee_bps: Option<u32>,
}

/// Parse a Uniswap V3 packed swap path: `token(20) | (fee(3) | token(20))+`.
/// Returns the first and last 20-byte addresses plus the *first* hop's fee.
/// Strict on length: `path.len() == 20 + 23*k` for some `k >= 1`.
pub(super) fn parse_v3_path(path: &[u8]) -> Result<ParsedV3Path, AdapterError> {
    const FEE_HOP_LEN: usize = 3 + ADDRESS_LEN; // 23 bytes per (fee, next-token) hop
    let min_len = ADDRESS_LEN + FEE_HOP_LEN; // single hop = 43 bytes
    if path.len() < min_len || !(path.len() - ADDRESS_LEN).is_multiple_of(FEE_HOP_LEN) {
        return Err(AdapterError::Invalid(format!(
            "Universal Router v3 path malformed: expected `addr(20) + (fee(3)+addr(20))+`, got {} bytes",
            path.len()
        )));
    }

    let token_in = address_from_bytes(&path[..ADDRESS_LEN])?;
    let token_out = address_from_bytes(&path[path.len() - ADDRESS_LEN..])?;
    let first_fee = (u32::from(path[20]) << 16) | (u32::from(path[21]) << 8) | u32::from(path[22]);

    Ok(ParsedV3Path {
        token_in,
        token_out,
        fee_bps: Some(first_fee / 100),
    })
}

pub(super) fn path_endpoints<'a>(
    path: &'a [Address],
    label: &str,
) -> Result<(&'a Address, &'a Address), AdapterError> {
    if path.len() < 2 {
        return Err(AdapterError::Invalid(format!(
            "Universal Router {label} path must contain at least 2 tokens"
        )));
    }
    Ok((&path[0], &path[path.len() - 1]))
}

pub(super) fn read_address_word(input: &[u8], word_index: usize) -> Result<Address, AdapterError> {
    let word = word_at(input, word_index)?;
    address_from_bytes(&word[WORD_LEN - ADDRESS_LEN..])
}

pub(super) fn read_decimal_word(input: &[u8], word_index: usize) -> Result<DecimalString, AdapterError> {
    decimal(&uint_decimal(word_at(input, word_index)?))
}

pub(super) fn read_bool_word(input: &[u8], word_index: usize) -> Result<bool, AdapterError> {
    let word = word_at(input, word_index)?;
    let value = word_as_usize(word)?;
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(AdapterError::Invalid(format!(
            "invalid ABI bool value {other}"
        ))),
    }
}

pub(super) fn read_dynamic_bytes(input: &[u8], offset_word_index: usize) -> Result<&[u8], AdapterError> {
    let offset = word_as_usize(word_at(input, offset_word_index)?)?;
    let length = word_as_usize(word_at_offset(input, offset)?)?;
    let start = offset
        .checked_add(WORD_LEN)
        .ok_or_else(|| AdapterError::Invalid("ABI bytes offset overflow".to_owned()))?;
    let end = start
        .checked_add(length)
        .ok_or_else(|| AdapterError::Invalid("ABI bytes length overflow".to_owned()))?;
    input
        .get(start..end)
        .ok_or_else(|| AdapterError::Invalid("ABI bytes out of bounds".to_owned()))
}

pub(super) fn read_dynamic_address_array(
    input: &[u8],
    offset_word_index: usize,
) -> Result<Vec<Address>, AdapterError> {
    let offset = word_as_usize(word_at(input, offset_word_index)?)?;
    let length = word_as_usize(word_at_offset(input, offset)?)?;
    let start = offset
        .checked_add(WORD_LEN)
        .ok_or_else(|| AdapterError::Invalid("ABI address[] offset overflow".to_owned()))?;

    (0..length)
        .map(|index| {
            let element_offset = start
                .checked_add(index * WORD_LEN)
                .ok_or_else(|| AdapterError::Invalid("ABI address[] offset overflow".to_owned()))?;
            let word = word_at_offset(input, element_offset)?;
            address_from_bytes(&word[WORD_LEN - ADDRESS_LEN..])
        })
        .collect()
}

fn word_at(input: &[u8], word_index: usize) -> Result<&[u8], AdapterError> {
    let offset = word_index
        .checked_mul(WORD_LEN)
        .ok_or_else(|| AdapterError::Invalid("ABI word offset overflow".to_owned()))?;
    word_at_offset(input, offset)
}

fn word_at_offset(input: &[u8], offset: usize) -> Result<&[u8], AdapterError> {
    let end = offset
        .checked_add(WORD_LEN)
        .ok_or_else(|| AdapterError::Invalid("ABI word offset overflow".to_owned()))?;
    input
        .get(offset..end)
        .ok_or_else(|| AdapterError::Invalid("ABI word out of bounds".to_owned()))
}

fn word_as_usize(word: &[u8]) -> Result<usize, AdapterError> {
    if word.len() != WORD_LEN {
        return Err(AdapterError::Invalid(format!(
            "expected ABI word length {WORD_LEN}, got {}",
            word.len()
        )));
    }
    if word[..24].iter().any(|byte| *byte != 0) {
        return Err(AdapterError::Invalid(
            "ABI word does not fit in usize".to_owned(),
        ));
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&word[24..]);
    usize::try_from(u64::from_be_bytes(bytes))
        .map_err(|e| AdapterError::Invalid(format!("ABI word does not fit in usize: {e}")))
}

pub(super) fn address_from_bytes(bytes: &[u8]) -> Result<Address, AdapterError> {
    if bytes.len() != ADDRESS_LEN {
        return Err(AdapterError::Invalid(format!(
            "expected address length {ADDRESS_LEN}, got {}",
            bytes.len()
        )));
    }
    Address::from_str(&format!("0x{}", hex::encode(bytes)))
        .map_err(|e| AdapterError::Invalid(format!("invalid address: {e}")))
}

pub(super) fn decimal(value: &str) -> Result<DecimalString, AdapterError> {
    DecimalString::from_str(value)
        .map_err(|e| AdapterError::Invalid(format!("invalid decimal string: {e}")))
}

pub(super) fn uint_decimal(word: &[u8]) -> String {
    let mut digits = vec![0u8];
    for byte in word {
        let mut carry = u16::from(*byte);
        for digit in digits.iter_mut().rev() {
            let value = u16::from(*digit) * 256 + carry;
            *digit = (value % 10) as u8;
            carry = value / 10;
        }
        while carry > 0 {
            digits.insert(0, (carry % 10) as u8);
            carry /= 10;
        }
    }

    digits
        .into_iter()
        .skip_while(|digit| *digit == 0)
        .map(|digit| char::from(b'0' + digit))
        .collect::<String>()
        .if_empty_then_zero()
}

trait EmptyDecimalExt {
    fn if_empty_then_zero(self) -> String;
}

impl EmptyDecimalExt for String {
    fn if_empty_then_zero(self) -> String {
        if self.is_empty() {
            "0".to_owned()
        } else {
            self
        }
    }
}

pub(super) fn policy_address_from_alloy(addr: &alloy_primitives::Address) -> Address {
    Address::from_str(&format!("0x{}", hex::encode(addr.0)))
        .expect("alloy address always parses as policy Address")
}
