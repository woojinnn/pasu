//! UR opcode `V4_SWAP` → one or more `Action::Swap` envelopes.
//!
//! V4_SWAP is itself an opcode stream: the per-opcode `inputs[i]` decodes as
//! `(bytes actions, bytes[] params)`, and the V4Router contract dispatches
//! `actions` byte-by-byte against [`V4_ROUTER_TABLE`]. The splitter
//! pre-decodes only the outer wrapper (actions + params live as bytes args
//! on the SubCall.decoded); this mapper does the inner dispatch and emits
//! one envelope per V4 *swap* action.
//!
//! V4 swap params don't carry a recipient — V4Router stages output as a
//! delta and a separate `TAKE` action drains it to the real recipient. We
//! run a two-pass walk:
//!   1. Collect a SwapAction envelope per swap action (default recipient =
//!      ctx.from) and capture the last TAKE recipient.
//!   2. Patch every swap envelope's recipient with the TAKE destination.
//!
//! TAKE_PORTION (dApp-fee) enrichment is intentionally not surfaced — the
//! current policy_engine schema doesn't carry a SwapEnrichment field.

use std::sync::Arc;

use abi_resolver::ids::UR_V4_SWAP_DECODER_ID;
use abi_resolver::subdecode::opcode_stream::{dispatch as dispatch_opcodes, DecodedStep};
use abi_resolver::subdecode::protocols::v4_router::V4_ROUTER_TABLE;
use abi_resolver::{DecodedCall, DecodedValue, DecoderId};
use alloy_dyn_abi::DynSolValue;
use alloy_primitives::U256;
use policy_engine::action::common::AmountKind;
use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::Address;

use crate::mapper::{MapContext, Mapper, MapperError, MapperId, MapperMatchKey};

use super::common::{
    asset_with_amount, decimal_from_uint, find_bytes, swap_amount_constraint, token_asset_ref,
};

pub const UR_V4_SWAP_MAPPER_ID: &str = "uniswap-ur/V4_SWAP";

// V4 inner-action opcodes dispatched against V4_ROUTER_TABLE.
const V4_ACTION_SWAP_EXACT_IN_SINGLE: u8 = 0x06;
const V4_ACTION_SWAP_EXACT_IN: u8 = 0x07;
const V4_ACTION_SWAP_EXACT_OUT_SINGLE: u8 = 0x08;
const V4_ACTION_SWAP_EXACT_OUT: u8 = 0x09;
const V4_ACTION_TAKE: u8 = 0x0e;

#[derive(Debug, Clone, Copy, Default)]
pub struct UrV4SwapMapper;

impl UrV4SwapMapper {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Mapper for UrV4SwapMapper {
    fn id(&self) -> MapperId {
        MapperId::new(UR_V4_SWAP_MAPPER_ID)
    }

    fn accepts(&self, decoded: &DecodedCall) -> bool {
        decoded.decoder_id.as_str() == UR_V4_SWAP_DECODER_ID
    }

    fn map(
        &self,
        ctx: &MapContext<'_>,
        decoded: &DecodedCall,
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let actions = find_bytes(decoded, "actions")?;
        let params = find_bytes_array(decoded, "params")?;
        let steps = dispatch_opcodes(&actions, &params, &V4_ROUTER_TABLE);

        // Pass 1 — build SwapAction per swap action; capture last TAKE recipient.
        let mut envelopes: Vec<ActionEnvelope> = Vec::new();
        let mut take_recipient: Option<Address> = None;
        for step in &steps {
            match step.opcode {
                V4_ACTION_SWAP_EXACT_IN_SINGLE => {
                    envelopes.push(decode_swap_single(ctx, step, SwapMode::ExactIn)?);
                }
                V4_ACTION_SWAP_EXACT_OUT_SINGLE => {
                    envelopes.push(decode_swap_single(ctx, step, SwapMode::ExactOut)?);
                }
                V4_ACTION_SWAP_EXACT_IN => {
                    envelopes.push(decode_swap_multi(ctx, step, SwapMode::ExactIn)?);
                }
                V4_ACTION_SWAP_EXACT_OUT => {
                    envelopes.push(decode_swap_multi(ctx, step, SwapMode::ExactOut)?);
                }
                V4_ACTION_TAKE => {
                    if let Some(r) = take_recipient_from(step) {
                        take_recipient = Some(r);
                    }
                }
                _ => {} // SETTLE / TAKE_PORTION / etc. — not swap-relevant here.
            }
        }

        // Pass 2 — patch the default ctx.from recipient with TAKE's destination.
        if let Some(real_recipient) = take_recipient.as_ref() {
            for env in &mut envelopes {
                let Action::Swap(s) = &mut env.action else {
                    continue;
                };
                if &s.recipient == ctx.from {
                    s.recipient = real_recipient.clone();
                }
            }
        }

        Ok(envelopes)
    }
}

#[must_use]
pub fn v4_swap_mapper_key() -> MapperMatchKey {
    MapperMatchKey {
        decoder_id: DecoderId::new(UR_V4_SWAP_DECODER_ID),
    }
}

#[must_use]
pub fn v4_swap_mapper_arc() -> Arc<dyn Mapper> {
    Arc::new(UrV4SwapMapper::new())
}

// ---------------------------------------------------------------------------
// V4 inner action helpers — work on the `DecodedStep` produced by
// dispatching `actions` against `V4_ROUTER_TABLE`. V4Router declares the
// opcode entries with a single tuple as their `input_signatures`, so
// `step.args` always has length 1 and the actual fields live inside the
// first arg's `DynSolValue::Tuple`.
// ---------------------------------------------------------------------------

/// `swapExactInSingle` / `swapExactOutSingle` —
/// `(poolKey, zeroForOne, amount, otherAmount, [minHopPriceX36], hookData)`.
fn decode_swap_single(
    ctx: &MapContext<'_>,
    step: &DecodedStep,
    mode: SwapMode,
) -> Result<ActionEnvelope, MapperError> {
    let fields = params_tuple(step)?;
    let pool_key_fields = tuple_value(fields.first().ok_or_else(|| invalid("poolKey missing"))?)?;
    let currency0 = tuple_address(&pool_key_fields[0], "poolKey.currency0")?;
    let currency1 = tuple_address(&pool_key_fields[1], "poolKey.currency1")?;
    let zero_for_one = tuple_bool(&fields[1], "zeroForOne")?;
    let amount = tuple_uint(&fields[2], "amount")?;
    let other_amount = tuple_uint(&fields[3], "otherAmount")?;
    let (token_in, token_out) = if zero_for_one {
        (currency0, currency1)
    } else {
        (currency1, currency0)
    };
    let fee_bps = extract_pool_fee_bps(pool_key_fields)?;
    Ok(build_swap_envelope(
        ctx,
        mode,
        &token_in,
        &token_out,
        amount,
        other_amount,
        fee_bps,
    ))
}

/// `swapExactIn` / `swapExactOut` (multi-hop) —
/// pre-#497: `(currency, path[], amountA, amountB)`
/// post-#497: `(currency, path[], minHopPriceX36[], amountA, amountB)`
fn decode_swap_multi(
    ctx: &MapContext<'_>,
    step: &DecodedStep,
    mode: SwapMode,
) -> Result<ActionEnvelope, MapperError> {
    let fields = params_tuple(step)?;
    let primary_currency = tuple_address(&fields[0], "currency")?;
    let path_items = match &fields[1] {
        DynSolValue::Array(items) => items,
        _ => return Err(invalid("V4 path not an array")),
    };
    let last = path_items.last().ok_or_else(|| invalid("V4 path empty"))?;
    let last_fields = tuple_value(last)?;
    let intermediate = tuple_address(&last_fields[0], "path.last.intermediateCurrency")?;
    let fee_bps = match &last_fields[1] {
        DynSolValue::Uint(u, _) => Some(u32::try_from(*u).unwrap_or(0) / 100),
        _ => None,
    };

    // post-#497 has 5 fields (extra minHopPriceX36[] at index 2);
    // mainnet has 4. Layout for both modes:
    //   ExactIn:  (currencyIn,  path[], [hop[]?,] amountIn,  amountOutMinimum)
    //   ExactOut: (currencyOut, path[], [hop[]?,] amountOut, amountInMaximum)
    let (a_idx, b_idx) = match fields.len() {
        5 => (3, 4),
        4 => (2, 3),
        n => {
            return Err(invalid(format!(
                "V4 multi-hop params expected 4 or 5 fields, got {n}"
            )))
        }
    };
    let amount_a = tuple_uint(&fields[a_idx], "amountPrimary")?;
    let amount_b = tuple_uint(&fields[b_idx], "amountSecondary")?;

    let (token_in, token_out) = match mode {
        // ExactOut path is reversed (output first in the packed path).
        SwapMode::ExactOut => (intermediate, primary_currency),
        // ExactIn — and the Market / Unknown fallbacks that shouldn't
        // surface for V4 dispatches but keep the match exhaustive.
        SwapMode::ExactIn | SwapMode::Market | SwapMode::Unknown => {
            (primary_currency, intermediate)
        }
    };
    Ok(build_swap_envelope(
        ctx, mode, &token_in, &token_out, amount_a, amount_b, fee_bps,
    ))
}

/// Build a SwapAction envelope from `(mode, token_in, token_out, amount_a,
/// amount_b)` where `amount_a`/`amount_b` semantics depend on `mode`:
///   - ExactIn:  amount_a = amountIn (Exact), amount_b = amountOutMin (Min)
///   - ExactOut: amount_a = amountOut (Exact), amount_b = amountInMax (Max)
fn build_swap_envelope(
    ctx: &MapContext<'_>,
    mode: SwapMode,
    token_in: &Address,
    token_out: &Address,
    amount_a: U256,
    amount_b: U256,
    fee_bps: Option<u32>,
) -> ActionEnvelope {
    // V4 swap actions are always ExactIn / ExactOut; the Market / Unknown
    // variants only exist for the generic SwapAction shape and shouldn't
    // surface from a V4 dispatch. Fall back to ExactIn semantics to keep
    // the match exhaustive without panicking — wrong AmountKind on a swap
    // that should never reach this branch is preferable to a crash.
    let (input_kind, output_kind, input_amt, output_amt) = match mode {
        SwapMode::ExactIn | SwapMode::Market | SwapMode::Unknown => (
            AmountKind::Exact,
            AmountKind::Min,
            decimal_from_uint(amount_a),
            decimal_from_uint(amount_b),
        ),
        SwapMode::ExactOut => (
            AmountKind::Max,
            AmountKind::Exact,
            decimal_from_uint(amount_b),
            decimal_from_uint(amount_a),
        ),
    };
    ActionEnvelope {
        category: Category::Dex,
        action: Action::Swap(SwapAction {
            swap_mode: mode,
            input_token: asset_with_amount(
                token_asset_ref(ctx, token_in),
                swap_amount_constraint(input_kind, input_amt),
            ),
            output_token: asset_with_amount(
                token_asset_ref(ctx, token_out),
                swap_amount_constraint(output_kind, output_amt),
            ),
            // Default recipient — patched in Pass 2 from TAKE if present.
            recipient: ctx.from.clone(),
            validity: None,
            fee_bps,
        }),
    }
}

// ---------------------------------------------------------------------------
// Decoded-step helpers (mirror what the call-adapter v4_actions module does
// for `DecodedStep`-shaped V4 action steps).
// ---------------------------------------------------------------------------

fn params_tuple(step: &DecodedStep) -> Result<&[DynSolValue], MapperError> {
    let args = step
        .args
        .as_ref()
        .ok_or_else(|| invalid(format!("V4 action {} carried no args", step.name)))?;
    let first = args
        .first()
        .ok_or_else(|| invalid(format!("V4 action {} args empty", step.name)))?;
    match &first.value {
        DynSolValue::Tuple(fields) => Ok(fields),
        other => Err(invalid(format!(
            "V4 action {} expected params tuple, got {other:?}",
            step.name
        ))),
    }
}

fn tuple_value(v: &DynSolValue) -> Result<&[DynSolValue], MapperError> {
    match v {
        DynSolValue::Tuple(fields) => Ok(fields),
        other => Err(invalid(format!("expected tuple, got {other:?}"))),
    }
}

fn tuple_address(v: &DynSolValue, name: &str) -> Result<Address, MapperError> {
    match v {
        DynSolValue::Address(addr) => {
            let hex = format!("0x{}", hex::encode(addr.0));
            hex.parse().map_err(|e| MapperError::ArgumentMismatch {
                name: name.into(),
                message: format!("invalid address: {e}"),
            })
        }
        other => Err(MapperError::ArgumentMismatch {
            name: name.into(),
            message: format!("expected address, got {other:?}"),
        }),
    }
}

fn tuple_bool(v: &DynSolValue, name: &str) -> Result<bool, MapperError> {
    match v {
        DynSolValue::Bool(b) => Ok(*b),
        other => Err(MapperError::ArgumentMismatch {
            name: name.into(),
            message: format!("expected bool, got {other:?}"),
        }),
    }
}

fn tuple_uint(v: &DynSolValue, name: &str) -> Result<U256, MapperError> {
    match v {
        DynSolValue::Uint(u, _) => Ok(*u),
        other => Err(MapperError::ArgumentMismatch {
            name: name.into(),
            message: format!("expected uint, got {other:?}"),
        }),
    }
}

fn extract_pool_fee_bps(pool_key: &[DynSolValue]) -> Result<Option<u32>, MapperError> {
    let fee = pool_key
        .get(2)
        .ok_or_else(|| invalid("V4 poolKey missing fee"))?;
    match fee {
        DynSolValue::Uint(u, _) => Ok(Some(u32::try_from(*u).unwrap_or(0) / 100)),
        _ => Ok(None),
    }
}

/// Pull `recipient` out of a TAKE step. Signature is
/// `(address currency, address recipient, uint256 amount)` — three flat
/// named args at the outer level (no `params` wrap), so step.args[1] is
/// already the recipient directly.
fn take_recipient_from(step: &DecodedStep) -> Option<Address> {
    let args = step.args.as_ref()?;
    let v = &args.get(1)?.value;
    let DynSolValue::Address(addr) = v else {
        return None;
    };
    format!("0x{}", hex::encode(addr.0)).parse().ok()
}

/// Look up a `bytes[]` arg by name and convert each element to `Vec<u8>`.
/// UR V4_SWAP carries `params: bytes[]` as the second arg.
fn find_bytes_array(decoded: &DecodedCall, name: &str) -> Result<Vec<Vec<u8>>, MapperError> {
    let arg = decoded
        .args
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| MapperError::MissingArgument(name.into()))?;
    let items = match &arg.value {
        DecodedValue::Array(items) => items,
        _ => {
            return Err(MapperError::ArgumentMismatch {
                name: name.into(),
                message: "expected bytes[] array".into(),
            })
        }
    };
    items
        .iter()
        .map(|v| match v {
            DecodedValue::Bytes(b) => Ok(b.clone()),
            _ => Err(MapperError::ArgumentMismatch {
                name: name.into(),
                message: "array entry must be bytes".into(),
            }),
        })
        .collect()
}

fn invalid<S: Into<String>>(msg: S) -> MapperError {
    MapperError::ArgumentMismatch {
        name: "V4_SWAP".into(),
        message: msg.into(),
    }
}
