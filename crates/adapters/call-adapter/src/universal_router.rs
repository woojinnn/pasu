use std::str::FromStr as _;

use abi_resolver::decoders::universal_router::UniversalRouterDecoder;
use abi_resolver::{DecodeContext, DecodedCall, DecodedValue, Decoder as _};
use policy_engine::action::dex::{SwapAction, SwapEnrichment, SwapMode};
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef, Category,
    DecimalString, Validity, ValiditySource,
};

use crate::{AdapterError, CallAdapter, CallAdapterId, CallContext};

const ADAPTER_ID: &str = "universal-router";
const WORD_LEN: usize = 32;
const ADDRESS_LEN: usize = 20;
const V3_SWAP_EXACT_IN: u8 = 0x00;
const V3_SWAP_EXACT_OUT: u8 = 0x01;
const V2_SWAP_EXACT_IN: u8 = 0x08;
const V2_SWAP_EXACT_OUT: u8 = 0x09;
const COMMAND_TYPE_MASK: u8 = 0x7f;
const ACTION_MSG_SENDER: &str = "0x0000000000000000000000000000000000000001";
const ACTION_ADDRESS_THIS: &str = "0x0000000000000000000000000000000000000002";

#[derive(Debug, Clone, Copy, Default)]
pub struct UniversalRouterCallAdapter {
    decoder: UniversalRouterDecoder,
}

impl UniversalRouterCallAdapter {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            decoder: UniversalRouterDecoder::new(),
        }
    }
}

impl CallAdapter for UniversalRouterCallAdapter {
    fn id(&self) -> CallAdapterId {
        CallAdapterId::new(ADAPTER_ID)
    }

    fn match_keys(&self) -> Vec<abi_resolver::CallMatchKey> {
        self.decoder.match_keys()
    }

    fn build(
        &self,
        ctx: &CallContext<'_>,
        calldata: &[u8],
    ) -> Result<Vec<ActionEnvelope>, AdapterError> {
        let dec_ctx = DecodeContext {
            chain_id: ctx.chain_id,
            to: ctx.to,
            value: ctx.value_wei,
            block_timestamp: ctx.block_timestamp,
        };
        let decoded = self.decoder.decode(&dec_ctx, calldata)?;
        let commands = bytes_arg(&decoded, "commands")?;
        let inputs = bytes_array_arg(&decoded, "inputs")?;
        let validity = validity_arg(&decoded)?;
        let mut envelopes = Vec::new();

        for (index, raw_opcode) in commands.iter().copied().enumerate() {
            let Some(input) = inputs.get(index) else {
                return Err(AdapterError::Invalid(format!(
                    "Universal Router missing input for command index {index}"
                )));
            };
            let opcode = raw_opcode & COMMAND_TYPE_MASK;
            match opcode {
                V3_SWAP_EXACT_IN => {
                    envelopes.push(decode_v3_swap_exact_in(ctx, input, validity.clone())?);
                }
                V3_SWAP_EXACT_OUT => {
                    envelopes.push(decode_v3_swap_exact_out(ctx, input, validity.clone())?);
                }
                V2_SWAP_EXACT_IN => {
                    envelopes.push(decode_v2_swap_exact_in(ctx, input, validity.clone())?);
                }
                V2_SWAP_EXACT_OUT => {
                    envelopes.push(decode_v2_swap_exact_out(ctx, input, validity.clone())?);
                }
                _ => {}
            }
        }

        Ok(envelopes)
    }
}

fn decode_v3_swap_exact_in(
    ctx: &CallContext<'_>,
    input: &[u8],
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let recipient = map_recipient(ctx, read_address_word(input, 0)?);
    let amount_in = read_decimal_word(input, 1)?;
    let amount_out_min = read_decimal_word(input, 2)?;
    let path = read_dynamic_bytes(input, 3)?;
    let _payer_is_user = read_bool_word(input, 4)?;
    let parsed_path = parse_v3_path(path)?;

    Ok(swap_envelope(SwapAction {
        swap_mode: SwapMode::ExactIn,
        token_in: asset_ref(ctx, &parsed_path.token_in),
        token_out: asset_ref(ctx, &parsed_path.token_out),
        amount_in: amount_constraint(AmountKind::Exact, amount_in),
        amount_out: amount_constraint(AmountKind::Min, amount_out_min),
        recipient,
        validity,
        fee_bps: parsed_path.fee_bps,
        enrichment: SwapEnrichment::default(),
    }))
}

fn decode_v3_swap_exact_out(
    ctx: &CallContext<'_>,
    input: &[u8],
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let recipient = map_recipient(ctx, read_address_word(input, 0)?);
    let amount_out = read_decimal_word(input, 1)?;
    let amount_in_max = read_decimal_word(input, 2)?;
    let path = read_dynamic_bytes(input, 3)?;
    let _payer_is_user = read_bool_word(input, 4)?;
    let parsed_path = parse_v3_path(path)?;

    // V3 exact-out paths are encoded in REVERSE order on Universal Router:
    // the path starts with the output token and ends with the input token,
    // because the swap router walks the path from the requested output side.
    // `parse_v3_path` always returns (first, fee, last) of the byte stream,
    // so for exact-out we flip the endpoints back into wallet-side semantics
    // (token_in = what the user spends, token_out = what they receive).
    Ok(swap_envelope(SwapAction {
        swap_mode: SwapMode::ExactOut,
        token_in: asset_ref(ctx, &parsed_path.token_out),
        token_out: asset_ref(ctx, &parsed_path.token_in),
        amount_in: amount_constraint(AmountKind::Max, amount_in_max),
        amount_out: amount_constraint(AmountKind::Exact, amount_out),
        recipient,
        validity,
        fee_bps: parsed_path.fee_bps,
        enrichment: SwapEnrichment::default(),
    }))
}

fn decode_v2_swap_exact_in(
    ctx: &CallContext<'_>,
    input: &[u8],
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let recipient = map_recipient(ctx, read_address_word(input, 0)?);
    let amount_in = read_decimal_word(input, 1)?;
    let amount_out_min = read_decimal_word(input, 2)?;
    let path = read_dynamic_address_array(input, 3)?;
    let _payer_is_user = read_bool_word(input, 4)?;
    let (token_in, token_out) = path_endpoints(&path, "v2")?;

    Ok(swap_envelope(SwapAction {
        swap_mode: SwapMode::ExactIn,
        token_in: asset_ref(ctx, token_in),
        token_out: asset_ref(ctx, token_out),
        amount_in: amount_constraint(AmountKind::Exact, amount_in),
        amount_out: amount_constraint(AmountKind::Min, amount_out_min),
        recipient,
        validity,
        fee_bps: Some(30),
        enrichment: SwapEnrichment::default(),
    }))
}

fn decode_v2_swap_exact_out(
    ctx: &CallContext<'_>,
    input: &[u8],
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let recipient = map_recipient(ctx, read_address_word(input, 0)?);
    let amount_out = read_decimal_word(input, 1)?;
    let amount_in_max = read_decimal_word(input, 2)?;
    let path = read_dynamic_address_array(input, 3)?;
    let _payer_is_user = read_bool_word(input, 4)?;
    let (token_in, token_out) = path_endpoints(&path, "v2")?;

    Ok(swap_envelope(SwapAction {
        swap_mode: SwapMode::ExactOut,
        token_in: asset_ref(ctx, token_in),
        token_out: asset_ref(ctx, token_out),
        amount_in: amount_constraint(AmountKind::Max, amount_in_max),
        amount_out: amount_constraint(AmountKind::Exact, amount_out),
        recipient,
        validity,
        fee_bps: Some(30),
        enrichment: SwapEnrichment::default(),
    }))
}

fn swap_envelope(action: SwapAction) -> ActionEnvelope {
    ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(action),
    }
}

fn amount_constraint(kind: AmountKind, value: DecimalString) -> AmountConstraint {
    AmountConstraint {
        kind,
        value: Some(value),
    }
}

fn asset_ref(ctx: &CallContext<'_>, address: &Address) -> AssetRef {
    let metadata = ctx.token_registry.lookup(ctx.chain_id, address);
    AssetRef {
        kind: AssetKind::Erc20,
        address: Some(address.clone()),
        token_id: None,
        symbol: metadata.as_ref().map(|m| m.symbol.clone()),
        decimals: metadata.map(|m| m.decimals),
    }
}

fn map_recipient(ctx: &CallContext<'_>, recipient: Address) -> Address {
    let recipient_text = recipient.to_string();
    if recipient_text == ACTION_MSG_SENDER {
        ctx.from.clone()
    } else if recipient_text == ACTION_ADDRESS_THIS {
        ctx.to.clone()
    } else {
        recipient
    }
}

struct ParsedV3Path {
    token_in: Address,
    token_out: Address,
    fee_bps: Option<u32>,
}

/// Parse a Uniswap V3 packed swap path: `token(20) | (fee(3) | token(20))+`.
/// Returns the first and last 20-byte addresses plus the *first* hop's fee.
/// Strict on length: `path.len() == 20 + 23*k` for some `k >= 1`.
fn parse_v3_path(path: &[u8]) -> Result<ParsedV3Path, AdapterError> {
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

fn path_endpoints<'a>(
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

fn bytes_arg<'a>(decoded: &'a DecodedCall, name: &str) -> Result<&'a [u8], AdapterError> {
    match decoded_arg(decoded, name)? {
        DecodedValue::Bytes(value) => Ok(value),
        other => Err(AdapterError::Invalid(format!(
            "expected {name} bytes, got {other:?}"
        ))),
    }
}

fn bytes_array_arg<'a>(
    decoded: &'a DecodedCall,
    name: &str,
) -> Result<Vec<&'a [u8]>, AdapterError> {
    match decoded_arg(decoded, name)? {
        DecodedValue::Array(values) => values
            .iter()
            .map(|value| match value {
                DecodedValue::Bytes(bytes) => Ok(bytes.as_slice()),
                other => Err(AdapterError::Invalid(format!(
                    "expected {name} bytes[] item, got {other:?}"
                ))),
            })
            .collect(),
        other => Err(AdapterError::Invalid(format!(
            "expected {name} bytes[], got {other:?}"
        ))),
    }
}

fn validity_arg(decoded: &DecodedCall) -> Result<Option<Validity>, AdapterError> {
    let Some(deadline) = decoded.args.iter().find(|arg| arg.name == "deadline") else {
        return Ok(None);
    };

    match &deadline.value {
        DecodedValue::Uint(value) => Ok(Some(Validity {
            expires_at: decimal(&value.to_string())?,
            source: ValiditySource::TxDeadline,
        })),
        other => Err(AdapterError::Invalid(format!(
            "expected deadline uint256, got {other:?}"
        ))),
    }
}

fn decoded_arg<'a>(decoded: &'a DecodedCall, name: &str) -> Result<&'a DecodedValue, AdapterError> {
    decoded
        .args
        .iter()
        .find(|arg| arg.name == name)
        .map(|arg| &arg.value)
        .ok_or_else(|| AdapterError::Invalid(format!("missing decoded argument {name}")))
}

fn read_address_word(input: &[u8], word_index: usize) -> Result<Address, AdapterError> {
    let word = word_at(input, word_index)?;
    address_from_bytes(&word[WORD_LEN - ADDRESS_LEN..])
}

fn read_decimal_word(input: &[u8], word_index: usize) -> Result<DecimalString, AdapterError> {
    decimal(&uint_decimal(word_at(input, word_index)?))
}

fn read_bool_word(input: &[u8], word_index: usize) -> Result<bool, AdapterError> {
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

fn read_dynamic_bytes(input: &[u8], offset_word_index: usize) -> Result<&[u8], AdapterError> {
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

fn read_dynamic_address_array(
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

fn address_from_bytes(bytes: &[u8]) -> Result<Address, AdapterError> {
    if bytes.len() != ADDRESS_LEN {
        return Err(AdapterError::Invalid(format!(
            "expected address length {ADDRESS_LEN}, got {}",
            bytes.len()
        )));
    }
    Address::from_str(&format!("0x{}", hex::encode(bytes)))
        .map_err(|e| AdapterError::Invalid(format!("invalid address: {e}")))
}

fn decimal(value: &str) -> Result<DecimalString, AdapterError> {
    DecimalString::from_str(value)
        .map_err(|e| AdapterError::Invalid(format!("invalid decimal string: {e}")))
}

fn uint_decimal(word: &[u8]) -> String {
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

#[cfg(test)]
mod tests {
    use crate::{CallAdapter as _, UniversalRouterCallAdapter};

    #[test]
    fn test_ur_call_adapter_match_keys() {
        assert!(!UniversalRouterCallAdapter::new().match_keys().is_empty());
    }
}
