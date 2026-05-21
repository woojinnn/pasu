//! Shared V4 swap envelope builder — the verified two-pass walk that turns a
//! dispatched V4 `Actions` step list into `Action::Swap` envelopes.
//!
//! Phase 7B (TB-2) extracted this from the legacy
//! [`v4_swap`](super::v4_swap) imperative mapper so the declarative
//! `opcode_stream_dispatch` path (`declarative::opcode_stream::
//! execute_v4_swap_step`) can reuse the exact same builder rather than
//! re-implementing it. Both callers first dispatch the inner
//! `(bytes actions, bytes[] params)` pair against
//! [`V4_ROUTER_TABLE`](abi_resolver::subdecode::protocols::v4_router::V4_ROUTER_TABLE)
//! — they differ only in how they obtain that pair (a `DecodedCall` arg vs. a
//! UR `DecodedStep`). This module takes the resulting `&[DecodedStep]` and is
//! agnostic to the entrypoint.
//!
//! ## Why a two-pass walk
//!
//! V4 swap action params do not carry a recipient — V4Router stages output
//! as a flash-accounting delta and a separate `TAKE` action drains it to the
//! real recipient (`Uniswap/v4-periphery @ main` `V4Router._handleAction`;
//! the `ExactInput*Params` / `ExactOutput*Params` structs in `IV4Router.sol`
//! have no `recipient` / `to` field). So:
//!
//!   1. Pass 1 — build one `SwapAction` envelope per swap action, with the
//!      recipient defaulted to `ctx.from`, and capture the last `TAKE`
//!      action's recipient.
//!   2. Pass 2 — patch every swap envelope still carrying the `ctx.from`
//!      default with the captured `TAKE` destination.
//!
//! When the stream ends in `TAKE_ALL` / `CLOSE_CURRENCY` / `CLEAR_OR_TAKE`
//! (no recipient arg — upstream takes to `msgSender()`), no patch happens and
//! the `ctx.from` default is the correct answer for a UR-routed swap.
//!
//! `TAKE_PORTION` (0x10, dApp-fee `bips`) is intentionally NOT treated as the
//! swap recipient — it skims a fee portion, not the swap output, and the
//! policy_engine `SwapAction` schema has no fee-enrichment field. Only `TAKE`
//! (0x0e) is consulted, matching the V4Router-routed swap flow.

use abi_resolver::subdecode::opcode_stream::DecodedStep;
use alloy_dyn_abi::DynSolValue;
use alloy_primitives::U256;
use policy_engine::action::common::AmountKind;
use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::Address;

use crate::mapper::{MapContext, MapperError};

use super::common::{
    asset_with_amount, decimal_from_uint, map_recipient, swap_amount_constraint, token_asset_ref,
};

// V4 inner-action opcodes dispatched against V4_ROUTER_TABLE.
const V4_ACTION_SWAP_EXACT_IN_SINGLE: u8 = 0x06;
const V4_ACTION_SWAP_EXACT_IN: u8 = 0x07;
const V4_ACTION_SWAP_EXACT_OUT_SINGLE: u8 = 0x08;
const V4_ACTION_SWAP_EXACT_OUT: u8 = 0x09;
const V4_ACTION_TAKE: u8 = 0x0e;

/// Build `Action::Swap` envelopes from an already-dispatched V4 `Actions`
/// step list (the output of dispatching `(actions, params)` against
/// `V4_ROUTER_TABLE`).
///
/// Returns one envelope per V4 *swap* action (`SWAP_EXACT_IN(_SINGLE)` /
/// `SWAP_EXACT_OUT(_SINGLE)`). A step list with no swap actions (e.g. a
/// settle/take-only stream, or pure liquidity actions) yields an empty
/// `Vec` — that is a clean "no swap intent" result, not a fault. A swap
/// action whose `params` blob failed Tier B ABI decoding surfaces a
/// `MapperError` so the caller can fall back rather than silently dropping a
/// swap.
pub fn build_v4_swap_envelopes(
    ctx: &MapContext<'_>,
    steps: &[DecodedStep],
) -> Result<Vec<ActionEnvelope>, MapperError> {
    // Pass 1 — build SwapAction per swap action; capture last TAKE recipient.
    let mut envelopes: Vec<ActionEnvelope> = Vec::new();
    let mut take_recipient: Option<Address> = None;
    for step in steps {
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
            _ => {} // SETTLE / TAKE_PORTION / liquidity actions — not swap-relevant here.
        }
    }

    // Pass 2 — patch the default ctx.from recipient with TAKE's destination.
    //
    // The TAKE `recipient` arg may be a V4-periphery `ActionConstants`
    // sentinel rather than a literal address: `MSG_SENDER` (`address(1)`)
    // and `ADDRESS_THIS` (`address(2)`), which `V4Router._mapRecipient`
    // resolves to `msgSender()` / the router itself. A UR-routed V4 swap
    // most commonly takes output to the swap initiator, encoded as
    // `address(1)`. Resolve through `map_recipient` (the shared UR
    // sentinel table — `0x..01 -> ctx.from`, `0x..02 -> ctx.to`) so the
    // envelope carries the real recipient, not the sentinel literal.
    if let Some(raw_recipient) = take_recipient.as_ref() {
        let mapped = map_recipient(ctx, raw_recipient.clone());
        for env in &mut envelopes {
            let Action::Swap(s) = &mut env.action else {
                continue;
            };
            if &s.recipient == ctx.from {
                s.recipient = mapped.clone();
            }
        }
    }

    Ok(envelopes)
}

// ---------------------------------------------------------------------------
// V4 inner action helpers — work on the `DecodedStep` produced by dispatching
// `actions` against `V4_ROUTER_TABLE`. V4Router declares the swap opcode
// entries with a single tuple as their `input_signatures`, so `step.args`
// always has length 1 and the actual fields live inside the first arg's
// `DynSolValue::Tuple`.
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
// Decoded-step helpers.
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

/// Pull the raw `recipient` out of a TAKE step. Signature is
/// `(address currency, address recipient, uint256 amount)` — three flat
/// named args at the outer level (no `params` wrap), so step.args[1] is
/// already the recipient directly. The returned address may be an
/// `ActionConstants` sentinel (`address(1)` / `address(2)`); the caller
/// (Pass 2 of [`build_v4_swap_envelopes`]) resolves it via
/// [`map_recipient`].
fn take_recipient_from(step: &DecodedStep) -> Option<Address> {
    let args = step.args.as_ref()?;
    let v = &args.get(1)?.value;
    let DynSolValue::Address(addr) = v else {
        return None;
    };
    format!("0x{}", hex::encode(addr.0)).parse().ok()
}

fn invalid<S: Into<String>>(msg: S) -> MapperError {
    MapperError::ArgumentMismatch {
        name: "V4_SWAP".into(),
        message: msg.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use abi_resolver::subdecode::opcode_stream::dispatch as dispatch_opcodes;
    use abi_resolver::subdecode::protocols::v4_router::V4_ROUTER_TABLE;
    use alloy_dyn_abi::JsonAbiExt;
    use alloy_json_abi::Function;
    use policy_engine::action::common::DecimalString;

    use crate::token_registry::EmptyTokenRegistry;

    use super::*;

    fn build_ctx<'a>(
        registry: &'a EmptyTokenRegistry,
        from: &'a Address,
        to: &'a Address,
        value: &'a DecimalString,
    ) -> MapContext<'a> {
        MapContext {
            chain_id: 1,
            from,
            to,
            value_wei: value,
            block_timestamp: Some(1_700_000_000),
            token_registry: registry,
            parent_calldata: None,
            depth: 0,
            resolver: None,
        }
    }

    fn dummy_addr(label: u8) -> Address {
        Address::from_str(&format!("0x{}{label:02x}", "0".repeat(38))).unwrap()
    }

    fn token_a() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48").unwrap()
    }

    fn token_b() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap()
    }

    fn take_dest() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0x5555555555555555555555555555555555555555").unwrap()
    }

    /// V4-periphery `ActionConstants.MSG_SENDER` — `address(1)`.
    fn sentinel_msg_sender() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0x0000000000000000000000000000000000000001").unwrap()
    }

    /// V4-periphery `ActionConstants.ADDRESS_THIS` — `address(2)`.
    fn sentinel_address_this() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0x0000000000000000000000000000000000000002").unwrap()
    }

    /// Encode a V4 `SWAP_EXACT_IN_SINGLE` (0x06) / `SWAP_EXACT_OUT_SINGLE`
    /// (0x08) params blob in the mainnet (pre-#497, no `minHopPriceX36`)
    /// shape.
    fn encode_swap_single_input(
        currency0: alloy_primitives::Address,
        currency1: alloy_primitives::Address,
        fee: u32,
        zero_for_one: bool,
        amount: u128,
        other_amount: u128,
    ) -> Vec<u8> {
        let pool_key = DynSolValue::Tuple(vec![
            DynSolValue::Address(currency0),
            DynSolValue::Address(currency1),
            DynSolValue::Uint(U256::from(fee), 24),
            DynSolValue::Int(alloy_primitives::I256::try_from(60).unwrap(), 24),
            DynSolValue::Address(alloy_primitives::Address::ZERO),
        ]);
        let params = DynSolValue::Tuple(vec![
            pool_key,
            DynSolValue::Bool(zero_for_one),
            DynSolValue::Uint(U256::from(amount), 128),
            DynSolValue::Uint(U256::from(other_amount), 128),
            DynSolValue::Bytes(vec![]),
        ]);
        let func = Function::parse(
            "step(((address,address,uint24,int24,address),bool,uint128,uint128,bytes))",
        )
        .unwrap();
        let raw = func.abi_encode_input(&[params]).unwrap();
        raw[4..].to_vec()
    }

    /// Encode a V4 `TAKE` (0x0e) params blob —
    /// `(address currency, address recipient, uint256 amount)`.
    fn encode_take_input(
        currency: alloy_primitives::Address,
        recipient: alloy_primitives::Address,
        amount: u128,
    ) -> Vec<u8> {
        let func = Function::parse("step(address,address,uint256)").unwrap();
        let raw = func
            .abi_encode_input(&[
                DynSolValue::Address(currency),
                DynSolValue::Address(recipient),
                DynSolValue::Uint(U256::from(amount), 256),
            ])
            .unwrap();
        raw[4..].to_vec()
    }

    /// Encode a V4 `SETTLE` (0x0b) params blob —
    /// `(address currency, uint256 amount, bool payerIsUser)`.
    fn encode_settle_input(
        currency: alloy_primitives::Address,
        amount: u128,
        payer_is_user: bool,
    ) -> Vec<u8> {
        let func = Function::parse("step(address,uint256,bool)").unwrap();
        let raw = func
            .abi_encode_input(&[
                DynSolValue::Address(currency),
                DynSolValue::Uint(U256::from(amount), 256),
                DynSolValue::Bool(payer_is_user),
            ])
            .unwrap();
        raw[4..].to_vec()
    }

    /// A `[SWAP_EXACT_IN_SINGLE, TAKE]` stream yields exactly one swap
    /// envelope, ExactIn, with the recipient patched from the TAKE step.
    #[test]
    fn swap_in_single_with_take_patches_recipient() {
        let actions = vec![0x06, 0x0e];
        let inputs = vec![
            encode_swap_single_input(token_a(), token_b(), 3_000, true, 1_000_000, 900_000),
            encode_take_input(token_b(), take_dest(), 900_000),
        ];
        let steps = dispatch_opcodes(&actions, &inputs, &V4_ROUTER_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_v4_swap_envelopes(&ctx, &steps).unwrap();
        assert_eq!(envelopes.len(), 1);
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap, got {:?}", envelopes[0].action);
        };
        assert_eq!(s.swap_mode, SwapMode::ExactIn);
        assert_eq!(s.input_token.amount.kind, AmountKind::Exact);
        assert_eq!(s.output_token.amount.kind, AmountKind::Min);
        // Recipient patched from TAKE — NOT the ctx.from default.
        assert_eq!(
            s.recipient.to_string(),
            format!("0x{}", hex::encode(take_dest()))
        );
    }

    /// Without a TAKE the swap envelope keeps the ctx.from default — correct
    /// for a stream ending in TAKE_ALL / CLOSE_CURRENCY (msgSender()).
    #[test]
    fn swap_without_take_defaults_recipient_to_ctx_from() {
        let actions = vec![0x06];
        let inputs =
            vec![encode_swap_single_input(token_a(), token_b(), 3_000, true, 1, 1)];
        let steps = dispatch_opcodes(&actions, &inputs, &V4_ROUTER_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_v4_swap_envelopes(&ctx, &steps).unwrap();
        assert_eq!(envelopes.len(), 1);
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap");
        };
        assert_eq!(s.recipient, from);
    }

    /// A settle/take-only stream (no swap action) yields zero envelopes —
    /// a clean "no swap intent" result, not a fault.
    #[test]
    fn settle_take_only_yields_zero_envelopes() {
        let actions = vec![0x0b, 0x0e];
        let inputs = vec![
            encode_settle_input(token_a(), 1_000_000, true),
            encode_take_input(token_b(), take_dest(), 900_000),
        ];
        let steps = dispatch_opcodes(&actions, &inputs, &V4_ROUTER_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_v4_swap_envelopes(&ctx, &steps).unwrap();
        assert!(envelopes.is_empty(), "settle/take-only must emit no swap envelopes");
    }

    /// `SWAP_EXACT_OUT_SINGLE` (0x08) yields an ExactOut swap envelope.
    #[test]
    fn swap_out_single_emits_exact_out() {
        let actions = vec![0x08];
        let inputs = vec![encode_swap_single_input(
            token_a(),
            token_b(),
            3_000,
            false,
            500_000,
            600_000,
        )];
        let steps = dispatch_opcodes(&actions, &inputs, &V4_ROUTER_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_v4_swap_envelopes(&ctx, &steps).unwrap();
        assert_eq!(envelopes.len(), 1);
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap");
        };
        assert_eq!(s.swap_mode, SwapMode::ExactOut);
        assert_eq!(s.input_token.amount.kind, AmountKind::Max);
        assert_eq!(s.output_token.amount.kind, AmountKind::Exact);
    }

    /// A swap action whose `params` blob fails Tier B ABI decoding (here a
    /// 1-byte malformed input) surfaces a `MapperError` rather than silently
    /// dropping the swap.
    #[test]
    fn malformed_swap_params_faults() {
        let actions = vec![0x06];
        let inputs = vec![vec![0x00]]; // too short to ABI-decode the struct.
        let steps = dispatch_opcodes(&actions, &inputs, &V4_ROUTER_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let err = build_v4_swap_envelopes(&ctx, &steps).unwrap_err();
        assert!(
            matches!(err, MapperError::ArgumentMismatch { .. }),
            "expected ArgumentMismatch, got {err:?}"
        );
    }

    /// Multiple swap actions in one stream — both get the same TAKE recipient.
    #[test]
    fn two_swaps_share_take_recipient() {
        let actions = vec![0x06, 0x06, 0x0e];
        let inputs = vec![
            encode_swap_single_input(token_a(), token_b(), 3_000, true, 1_000, 900),
            encode_swap_single_input(token_b(), token_a(), 500, false, 2_000, 1_800),
            encode_take_input(token_a(), take_dest(), 1_800),
        ];
        let steps = dispatch_opcodes(&actions, &inputs, &V4_ROUTER_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_v4_swap_envelopes(&ctx, &steps).unwrap();
        assert_eq!(envelopes.len(), 2);
        for env in &envelopes {
            let Action::Swap(s) = &env.action else {
                panic!("expected Swap");
            };
            assert_eq!(
                s.recipient.to_string(),
                format!("0x{}", hex::encode(take_dest()))
            );
        }
    }

    /// A TAKE `recipient` of the `ActionConstants.MSG_SENDER` sentinel
    /// (`address(1)`) must resolve to `ctx.from` — V4Router routes the
    /// output to `msgSender()`. The envelope must carry the real address,
    /// not the `0x..01` literal.
    #[test]
    fn take_sentinel_msg_sender_maps_to_ctx_from() {
        let actions = vec![0x06, 0x0e];
        let inputs = vec![
            encode_swap_single_input(token_a(), token_b(), 3_000, true, 1_000_000, 900_000),
            encode_take_input(token_b(), sentinel_msg_sender(), 900_000),
        ];
        let steps = dispatch_opcodes(&actions, &inputs, &V4_ROUTER_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_v4_swap_envelopes(&ctx, &steps).unwrap();
        assert_eq!(envelopes.len(), 1);
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap");
        };
        // Sentinel address(1) resolved to ctx.from — NOT the 0x..01 literal.
        assert_eq!(s.recipient, from);
        assert_ne!(
            s.recipient.to_string(),
            format!("0x{}", hex::encode(sentinel_msg_sender()))
        );
    }

    /// A TAKE `recipient` of the `ActionConstants.ADDRESS_THIS` sentinel
    /// (`address(2)`) must resolve to `ctx.to` — the router itself.
    #[test]
    fn take_sentinel_address_this_maps_to_ctx_to() {
        let actions = vec![0x06, 0x0e];
        let inputs = vec![
            encode_swap_single_input(token_a(), token_b(), 3_000, true, 1_000_000, 900_000),
            encode_take_input(token_b(), sentinel_address_this(), 900_000),
        ];
        let steps = dispatch_opcodes(&actions, &inputs, &V4_ROUTER_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_v4_swap_envelopes(&ctx, &steps).unwrap();
        assert_eq!(envelopes.len(), 1);
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap");
        };
        // Sentinel address(2) resolved to ctx.to (the router) — NOT 0x..02.
        assert_eq!(s.recipient, to);
        assert_ne!(
            s.recipient.to_string(),
            format!("0x{}", hex::encode(sentinel_address_this()))
        );
    }
}
