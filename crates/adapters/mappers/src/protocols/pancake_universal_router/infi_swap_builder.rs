//! Shared Pancake Infinity `INFI_SWAP` envelope builder — the two-pass walk
//! that turns a dispatched Pancake Infinity `Actions` step list into
//! `Action::Swap` envelopes.
//!
//! Mirrors [`super::super::universal_router::v4_swap_builder::build_v4_swap_envelopes`]
//! (the Uniswap V4 counterpart) but accounts for two protocol divergences
//! catalogued under D010:
//!
//! 1. **`PoolKey` layout** — Pancake Infinity (`pancakeswap/infinity-core @
//!    main` `src/types/PoolKey.sol`) is **6 fields**:
//!    `(currency0, currency1, hooks, poolManager, fee, parameters)`.
//!    Uniswap V4 (`Uniswap/v4-core` `src/types/PoolKey.sol`) is **5 fields**:
//!    `(currency0, currency1, fee, tickSpacing, hooks)`.
//!    The `fee` index is therefore **4** on Pancake (vs **2** on V4).
//!
//! 2. **`PathKey` layout** — Pancake Infinity (`pancakeswap/infinity-periphery
//!    @ main` `src/libraries/PathKey.sol`) is **6 fields**:
//!    `(intermediateCurrency, fee, hooks, poolManager, hookData, parameters)`.
//!    V4 path entries are 5 fields with no `poolManager` / `parameters`.
//!    The `intermediateCurrency` (index 0) and `fee` (index 1) positions are
//!    the same, so multi-hop endpoint / fee extraction can share the
//!    relative-index logic.
//!
//! ## Action-table coverage
//!
//! Pancake Infinity periphery's `Actions.sol` lays out **two** swap families:
//!
//! * **CL pool** (concentrated liquidity, V4-like)
//!     - `CL_SWAP_EXACT_IN_SINGLE` = `0x06`
//!     - `CL_SWAP_EXACT_IN`        = `0x07`
//!     - `CL_SWAP_EXACT_OUT_SINGLE`= `0x08`
//!     - `CL_SWAP_EXACT_OUT`       = `0x09`
//! * **Bin pool** (LB/TraderJoe-style, periphery-only on Pancake)
//!     - `BIN_SWAP_EXACT_IN_SINGLE`= `0x1c`
//!     - `BIN_SWAP_EXACT_IN`       = `0x1d`
//!     - `BIN_SWAP_EXACT_OUT_SINGLE`= `0x1e`
//!     - `BIN_SWAP_EXACT_OUT`      = `0x1f`
//!
//! The CL variants carry a `zeroForOne` bool; the Bin variants carry a
//! `swapForY` bool with the same semantics for input/output token selection.
//!
//! ## Two-pass walk
//!
//! Identical in shape to the V4 builder — Pancake Infinity periphery's
//! `InfinityRouter._handleAction` (`pancakeswap/infinity-periphery @ main`
//! `src/InfinityRouter.sol`) stages each swap's output as a flash-accounting
//! delta and drains it via a separate `TAKE` (0x0e). Swap param structs in
//! `ICLRouterBase.sol` / `IBinRouterBase.sol` have no `recipient` field, so we
//! patch each swap envelope's recipient from the trailing `TAKE` step in pass
//! 2. Pancake Infinity reuses the V4 `ActionConstants` sentinel addresses
//! (`MSG_SENDER = 0x...01`, `ADDRESS_THIS = 0x...02`) — `map_recipient`
//! resolves both.

use abi_resolver::subdecode::opcode_stream::DecodedStep;
use alloy_dyn_abi::DynSolValue;
use alloy_primitives::U256;
use policy_engine::action::common::AmountKind;
use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::action::envelope::{Action, ActionEnvelope, Category};
use policy_engine::action::Address;

use crate::mapper::{MapContext, MapperError};

use super::super::universal_router::common::{
    asset_with_amount, decimal_from_uint, map_recipient, swap_amount_constraint, token_asset_ref,
};

// CL pool swap opcodes (same byte values as V4Router; the dispatch table is
// PANCAKE_INFI_TABLE, not V4_ROUTER_TABLE, so the schemas underneath differ).
const INFI_ACTION_CL_SWAP_EXACT_IN_SINGLE: u8 = 0x06;
const INFI_ACTION_CL_SWAP_EXACT_IN: u8 = 0x07;
const INFI_ACTION_CL_SWAP_EXACT_OUT_SINGLE: u8 = 0x08;
const INFI_ACTION_CL_SWAP_EXACT_OUT: u8 = 0x09;
// Bin pool swap opcodes (Pancake-specific; V4Router leaves these unused).
const INFI_ACTION_BIN_SWAP_EXACT_IN_SINGLE: u8 = 0x1c;
const INFI_ACTION_BIN_SWAP_EXACT_IN: u8 = 0x1d;
const INFI_ACTION_BIN_SWAP_EXACT_OUT_SINGLE: u8 = 0x1e;
const INFI_ACTION_BIN_SWAP_EXACT_OUT: u8 = 0x1f;
// Shared settlement opcode — same byte / shape as V4Router TAKE.
const INFI_ACTION_TAKE: u8 = 0x0e;

/// Build `Action::Swap` envelopes from an already-dispatched Pancake Infinity
/// `Actions` step list (the output of dispatching `(actions, params)` against
/// `PANCAKE_INFI_TABLE`).
///
/// Returns one envelope per Infinity *swap* action (CL `SWAP_EXACT_*` or Bin
/// `SWAP_EXACT_*`). A step list with no swap actions (e.g. a settle/take-only
/// stream, or pure liquidity actions) yields an empty `Vec` — that is a clean
/// "no swap intent" result, not a fault. A swap action whose `params` blob
/// failed Tier B ABI decoding surfaces a `MapperError` so the caller can fall
/// back rather than silently dropping a swap.
pub fn build_pancake_infi_swap_envelopes(
    ctx: &MapContext<'_>,
    steps: &[DecodedStep],
) -> Result<Vec<ActionEnvelope>, MapperError> {
    // Pass 1 — build SwapAction per swap action; capture last TAKE recipient.
    let mut envelopes: Vec<ActionEnvelope> = Vec::new();
    let mut take_recipient: Option<Address> = None;
    for step in steps {
        match step.opcode {
            // CL single-hop: zeroForOne carries the direction.
            INFI_ACTION_CL_SWAP_EXACT_IN_SINGLE => {
                envelopes.push(decode_swap_single(ctx, step, SwapMode::ExactIn)?);
            }
            INFI_ACTION_CL_SWAP_EXACT_OUT_SINGLE => {
                envelopes.push(decode_swap_single(ctx, step, SwapMode::ExactOut)?);
            }
            // CL multi-hop.
            INFI_ACTION_CL_SWAP_EXACT_IN => {
                envelopes.push(decode_swap_multi(ctx, step, SwapMode::ExactIn)?);
            }
            INFI_ACTION_CL_SWAP_EXACT_OUT => {
                envelopes.push(decode_swap_multi(ctx, step, SwapMode::ExactOut)?);
            }
            // Bin single-hop: swapForY plays the same role as CL's zeroForOne.
            INFI_ACTION_BIN_SWAP_EXACT_IN_SINGLE => {
                envelopes.push(decode_swap_single(ctx, step, SwapMode::ExactIn)?);
            }
            INFI_ACTION_BIN_SWAP_EXACT_OUT_SINGLE => {
                envelopes.push(decode_swap_single(ctx, step, SwapMode::ExactOut)?);
            }
            // Bin multi-hop.
            INFI_ACTION_BIN_SWAP_EXACT_IN => {
                envelopes.push(decode_swap_multi(ctx, step, SwapMode::ExactIn)?);
            }
            INFI_ACTION_BIN_SWAP_EXACT_OUT => {
                envelopes.push(decode_swap_multi(ctx, step, SwapMode::ExactOut)?);
            }
            INFI_ACTION_TAKE => {
                if let Some(r) = take_recipient_from(step) {
                    take_recipient = Some(r);
                }
            }
            _ => {} // SETTLE / TAKE_PORTION / TAKE_ALL / liquidity actions —
                    // not swap-relevant for envelope emission here.
        }
    }

    // Pass 2 — patch the default ctx.from recipient with TAKE's destination.
    // Pancake Infinity reuses the V4-periphery `ActionConstants` sentinel
    // table (`MSG_SENDER = 0x..01`, `ADDRESS_THIS = 0x..02`), so the shared
    // UR sentinel resolver `map_recipient` is the correct mapping path.
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
// Inner action helpers — work on the `DecodedStep` produced by dispatching
// `actions` against `PANCAKE_INFI_TABLE`. The table declares each swap opcode
// with a single-tuple `input_json_abi` (the swap param struct is dynamic via
// `bytes hookData`), so `step.args[0].value` is the `DynSolValue::Tuple`
// carrying the swap fields.
// ---------------------------------------------------------------------------

/// `CL_SWAP_EXACT_IN_SINGLE` / `CL_SWAP_EXACT_OUT_SINGLE`
/// (and `BIN_SWAP_*_SINGLE` — identical layout except the second bool is
/// `swapForY` instead of `zeroForOne`):
///
/// `(PoolKey poolKey, bool direction, uint128 amount, uint128 otherAmount,
///   bytes hookData)`
///
/// PoolKey (Pancake Infinity 6 fields):
/// `[0] currency0`, `[1] currency1`, `[2] hooks`, `[3] poolManager`,
/// `[4] fee`, `[5] parameters`.
fn decode_swap_single(
    ctx: &MapContext<'_>,
    step: &DecodedStep,
    mode: SwapMode,
) -> Result<ActionEnvelope, MapperError> {
    let fields = params_tuple(step)?;
    let pool_key_fields = tuple_value(fields.first().ok_or_else(|| invalid("poolKey missing"))?)?;
    if pool_key_fields.len() < 6 {
        return Err(invalid(format!(
            "Pancake Infinity PoolKey expected 6 fields, got {}",
            pool_key_fields.len()
        )));
    }
    let currency0 = tuple_address(&pool_key_fields[0], "poolKey.currency0")?;
    let currency1 = tuple_address(&pool_key_fields[1], "poolKey.currency1")?;
    let direction = tuple_bool(&fields[1], "direction")?;
    let amount = tuple_uint(&fields[2], "amount")?;
    let other_amount = tuple_uint(&fields[3], "otherAmount")?;
    let (token_in, token_out) = if direction {
        // zeroForOne == true   → currency0 → currency1
        // swapForY  == true    → currency0 → currency1 (X = currency0, Y = currency1)
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

/// `CL_SWAP_EXACT_IN` / `CL_SWAP_EXACT_OUT` / Bin counterparts —
/// `(Currency primaryCurrency, PathKey[] path, uint128 amountA, uint128 amountB)`
///
/// Pancake Infinity router has **no** `minHopPriceX36[]` array (Uniswap V4Router
/// post-#497 grew that trailing slot; Pancake periphery did not).
///
/// PathKey (6 fields):
/// `[0] intermediateCurrency`, `[1] fee`, `[2] hooks`, `[3] poolManager`,
/// `[4] hookData`, `[5] parameters`.
fn decode_swap_multi(
    ctx: &MapContext<'_>,
    step: &DecodedStep,
    mode: SwapMode,
) -> Result<ActionEnvelope, MapperError> {
    let fields = params_tuple(step)?;
    if fields.len() < 4 {
        return Err(invalid(format!(
            "Pancake Infinity multi-hop params expected 4 fields, got {}",
            fields.len()
        )));
    }
    let primary_currency = tuple_address(&fields[0], "currency")?;
    let path_items = match &fields[1] {
        DynSolValue::Array(items) => items,
        _ => return Err(invalid("Pancake Infinity path not an array")),
    };
    let last = path_items
        .last()
        .ok_or_else(|| invalid("Pancake Infinity path empty"))?;
    let last_fields = tuple_value(last)?;
    if last_fields.len() < 2 {
        return Err(invalid(format!(
            "Pancake Infinity PathKey expected ≥ 2 fields, got {}",
            last_fields.len()
        )));
    }
    let intermediate = tuple_address(&last_fields[0], "path.last.intermediateCurrency")?;
    // PathKey.fee at index 1 (same as V4) — fee is uint24, expressed in
    // hundredths of bps. Divide by 100 → normalised bps.
    let fee_bps = match &last_fields[1] {
        DynSolValue::Uint(u, _) => Some(u32::try_from(*u).unwrap_or(0) / 100),
        _ => None,
    };

    // Pancake Infinity always has 4 fields here — no minHopPriceX36 slot.
    // Layout for both modes:
    //   ExactIn:  (currencyIn,  path[], amountIn,  amountOutMinimum)
    //   ExactOut: (currencyOut, path[], amountOut, amountInMaximum)
    let amount_a = tuple_uint(&fields[2], "amountPrimary")?;
    let amount_b = tuple_uint(&fields[3], "amountSecondary")?;

    let (token_in, token_out) = match mode {
        // ExactOut path is reversed (output first in the packed path).
        SwapMode::ExactOut => (intermediate, primary_currency),
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
// Decoded-step helpers — identical pattern to v4_swap_builder; kept local so
// the V4-specific MapperError diagnostic wording can diverge if needed.
// ---------------------------------------------------------------------------

fn params_tuple(step: &DecodedStep) -> Result<&[DynSolValue], MapperError> {
    let args = step
        .args
        .as_ref()
        .ok_or_else(|| invalid(format!("Pancake Infi action {} carried no args", step.name)))?;
    let first = args
        .first()
        .ok_or_else(|| invalid(format!("Pancake Infi action {} args empty", step.name)))?;
    match &first.value {
        DynSolValue::Tuple(fields) => Ok(fields),
        other => Err(invalid(format!(
            "Pancake Infi action {} expected params tuple, got {other:?}",
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

/// Pancake Infinity PoolKey.fee is at index **4** (D010 — Uniswap V4 is at
/// index 2). Returns the fee in normalised bps (uint24 stores hundredths of a
/// bp; divide by 100).
fn extract_pool_fee_bps(pool_key: &[DynSolValue]) -> Result<Option<u32>, MapperError> {
    let fee = pool_key
        .get(4)
        .ok_or_else(|| invalid("Pancake Infinity PoolKey missing fee (index 4)"))?;
    match fee {
        DynSolValue::Uint(u, _) => Ok(Some(u32::try_from(*u).unwrap_or(0) / 100)),
        _ => Ok(None),
    }
}

/// Pull the raw `recipient` out of a TAKE step. PANCAKE_INFI_TABLE declares
/// `TAKE` with `input_signatures = ["(address currency, address recipient,
/// uint256 amount)"]` (3 flat named args, no `params` wrap), so
/// `step.args[1]` is the recipient. The returned address may be a V4-
/// periphery `ActionConstants` sentinel (`address(1)` / `address(2)`); the
/// caller (Pass 2 of [`build_pancake_infi_swap_envelopes`]) resolves it via
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
        name: "INFI_SWAP".into(),
        message: msg.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use abi_resolver::subdecode::opcode_stream::dispatch as dispatch_opcodes;
    use abi_resolver::subdecode::protocols::pancake_infinity::PANCAKE_INFI_TABLE;
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
            chain_id: 8453,
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
        // Base USDC.
        alloy_primitives::Address::from_str("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()
    }

    fn token_b() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0xfde4c96c8593536e31f229ea8f37b2ada2699bb2").unwrap()
    }

    fn take_dest() -> alloy_primitives::Address {
        // Real EOA recipient from the D008 corpus tx.
        alloy_primitives::Address::from_str("0x5111af449018903bb05783618bfa64d7b131213a").unwrap()
    }

    fn pool_manager() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0xa0ffb9c1ce1fe56963b0321b32e7a0302114058b").unwrap()
    }

    /// V4-periphery `ActionConstants.MSG_SENDER` — `address(1)` — reused by
    /// Pancake Infinity periphery.
    fn sentinel_msg_sender() -> alloy_primitives::Address {
        alloy_primitives::Address::from_str("0x0000000000000000000000000000000000000001").unwrap()
    }

    /// Encode a Pancake Infinity `CL_SWAP_EXACT_IN_SINGLE` (0x06) params
    /// blob — the 6-field PoolKey + 4 flat fields wrapped in an outer tuple
    /// (matches `PANCAKE_INFI_TABLE` JSON ABI for opcode 0x06).
    #[allow(clippy::too_many_arguments)]
    fn encode_cl_swap_single_input(
        currency0: alloy_primitives::Address,
        currency1: alloy_primitives::Address,
        hooks: alloy_primitives::Address,
        pool_manager: alloy_primitives::Address,
        fee: u32,
        zero_for_one: bool,
        amount: u128,
        other_amount: u128,
    ) -> Vec<u8> {
        let pool_key = DynSolValue::Tuple(vec![
            DynSolValue::Address(currency0),
            DynSolValue::Address(currency1),
            DynSolValue::Address(hooks),
            DynSolValue::Address(pool_manager),
            DynSolValue::Uint(U256::from(fee), 24),
            DynSolValue::FixedBytes(alloy_primitives::B256::ZERO, 32),
        ]);
        let params = DynSolValue::Tuple(vec![
            pool_key,
            DynSolValue::Bool(zero_for_one),
            DynSolValue::Uint(U256::from(amount), 128),
            DynSolValue::Uint(U256::from(other_amount), 128),
            DynSolValue::Bytes(vec![]),
        ]);
        let func = Function::parse(
            "step(((address,address,address,address,uint24,bytes32),bool,uint128,uint128,bytes))",
        )
        .unwrap();
        let raw = func.abi_encode_input(&[params]).unwrap();
        raw[4..].to_vec()
    }

    /// Encode `TAKE` (0x0e) params — `(address currency, address recipient,
    /// uint256 amount)`. Same shape as V4Router TAKE.
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

    /// Encode `SETTLE` (0x0b) params — `(address currency, uint256 amount,
    /// bool payerIsUser)`. Same shape as V4Router SETTLE.
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

    /// `[CL_SWAP_EXACT_IN_SINGLE, SETTLE, TAKE]` — the action triple from the
    /// D008 corpus tx (Base UR `0x86a47a52`). Builder must emit exactly one
    /// CL swap envelope with the recipient patched from TAKE.
    #[test]
    fn d008_realtx_cl_swap_in_settle_take_emits_one_envelope() {
        let actions = vec![0x06, 0x0b, 0x0e];
        let inputs = vec![
            encode_cl_swap_single_input(
                token_a(),
                token_b(),
                alloy_primitives::Address::ZERO,
                pool_manager(),
                0x100000,
                true,
                0x5160144d,
                0x50d173ff,
            ),
            encode_settle_input(token_a(), 0x5160144d, true),
            encode_take_input(token_b(), take_dest(), 1),
        ];
        let steps = dispatch_opcodes(&actions, &inputs, &PANCAKE_INFI_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_pancake_infi_swap_envelopes(&ctx, &steps).unwrap();
        assert_eq!(
            envelopes.len(),
            1,
            "D008 regression — expected exactly one swap envelope from CL_SWAP+SETTLE+TAKE"
        );
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap, got {:?}", envelopes[0].action);
        };
        assert_eq!(s.swap_mode, SwapMode::ExactIn);
        // Recipient patched from TAKE.
        assert_eq!(
            s.recipient.to_string(),
            format!("0x{}", hex::encode(take_dest()))
        );
    }

    /// `SWAP_EXACT_OUT_SINGLE` (0x08) — ExactOut with the input amount
    /// constraint = max, output = exact.
    #[test]
    fn cl_swap_out_single_emits_exact_out() {
        let actions = vec![0x08];
        let inputs = vec![encode_cl_swap_single_input(
            token_a(),
            token_b(),
            alloy_primitives::Address::ZERO,
            pool_manager(),
            3_000,
            false,
            500_000,
            600_000,
        )];
        let steps = dispatch_opcodes(&actions, &inputs, &PANCAKE_INFI_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_pancake_infi_swap_envelopes(&ctx, &steps).unwrap();
        assert_eq!(envelopes.len(), 1);
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap");
        };
        assert_eq!(s.swap_mode, SwapMode::ExactOut);
        assert_eq!(s.input_token.amount.kind, AmountKind::Max);
        assert_eq!(s.output_token.amount.kind, AmountKind::Exact);
    }

    /// `BIN_SWAP_EXACT_IN_SINGLE` (0x1c) — same shape as CL, different opcode.
    /// `swapForY=true` ⇒ currency0 → currency1 (X→Y).
    #[test]
    fn bin_swap_in_single_emits_one_envelope() {
        let actions = vec![0x1c];
        let inputs = vec![encode_cl_swap_single_input(
            token_a(),
            token_b(),
            alloy_primitives::Address::ZERO,
            pool_manager(),
            2_500,
            true,
            1_000_000,
            900_000,
        )];
        let steps = dispatch_opcodes(&actions, &inputs, &PANCAKE_INFI_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_pancake_infi_swap_envelopes(&ctx, &steps).unwrap();
        assert_eq!(envelopes.len(), 1);
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap");
        };
        assert_eq!(s.swap_mode, SwapMode::ExactIn);
        // fee_bps = 2500 / 100 = 25 bps.
        assert_eq!(s.fee_bps, Some(25));
    }

    /// A settle/take-only stream (no swap action) emits zero envelopes — a
    /// clean "no swap intent" result, not a fault.
    #[test]
    fn settle_take_only_yields_zero_envelopes() {
        let actions = vec![0x0b, 0x0e];
        let inputs = vec![
            encode_settle_input(token_a(), 1_000_000, true),
            encode_take_input(token_b(), take_dest(), 900_000),
        ];
        let steps = dispatch_opcodes(&actions, &inputs, &PANCAKE_INFI_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_pancake_infi_swap_envelopes(&ctx, &steps).unwrap();
        assert!(
            envelopes.is_empty(),
            "settle/take-only must emit no swap envelopes"
        );
    }

    /// Two swap actions in one stream — both inherit the same TAKE recipient.
    #[test]
    fn two_swaps_share_take_recipient() {
        let actions = vec![0x06, 0x06, 0x0e];
        let inputs = vec![
            encode_cl_swap_single_input(
                token_a(),
                token_b(),
                alloy_primitives::Address::ZERO,
                pool_manager(),
                3_000,
                true,
                1_000,
                900,
            ),
            encode_cl_swap_single_input(
                token_b(),
                token_a(),
                alloy_primitives::Address::ZERO,
                pool_manager(),
                500,
                false,
                2_000,
                1_800,
            ),
            encode_take_input(token_a(), take_dest(), 1_800),
        ];
        let steps = dispatch_opcodes(&actions, &inputs, &PANCAKE_INFI_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_pancake_infi_swap_envelopes(&ctx, &steps).unwrap();
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

    /// TAKE recipient = `MSG_SENDER` sentinel (`address(1)`) → resolved to
    /// `ctx.from`, NOT the `0x..01` literal.
    #[test]
    fn take_sentinel_msg_sender_maps_to_ctx_from() {
        let actions = vec![0x06, 0x0e];
        let inputs = vec![
            encode_cl_swap_single_input(
                token_a(),
                token_b(),
                alloy_primitives::Address::ZERO,
                pool_manager(),
                3_000,
                true,
                1_000_000,
                900_000,
            ),
            encode_take_input(token_b(), sentinel_msg_sender(), 900_000),
        ];
        let steps = dispatch_opcodes(&actions, &inputs, &PANCAKE_INFI_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_pancake_infi_swap_envelopes(&ctx, &steps).unwrap();
        assert_eq!(envelopes.len(), 1);
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap");
        };
        assert_eq!(s.recipient, from);
        assert_ne!(
            s.recipient.to_string(),
            format!("0x{}", hex::encode(sentinel_msg_sender()))
        );
    }

    /// A malformed `params` blob surfaces as `MapperError::ArgumentMismatch`
    /// rather than silently dropping the swap. Mirrors the V4 builder's
    /// `malformed_swap_params_faults` guard.
    #[test]
    fn malformed_swap_params_faults() {
        let actions = vec![0x06];
        let inputs = vec![vec![0x00]]; // too short to ABI-decode the tuple.
        let steps = dispatch_opcodes(&actions, &inputs, &PANCAKE_INFI_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let err = build_pancake_infi_swap_envelopes(&ctx, &steps).unwrap_err();
        assert!(
            matches!(err, MapperError::ArgumentMismatch { .. }),
            "expected ArgumentMismatch, got {err:?}"
        );
    }

    /// Regression guard for D010 — the Pancake Infinity PoolKey lays `fee`
    /// out at **index 4**. Confirming a non-zero fee value at that slot is
    /// the one extracted by [`extract_pool_fee_bps`].
    #[test]
    fn poolkey_fee_extracted_from_index_four_not_two() {
        // Construct a PoolKey where index 2 (hooks slot) is a non-zero
        // address (which used to silently mis-bind as fee with a V4 mirror)
        // and index 4 is the real fee — 3000 / 100 = 30 bps.
        let actions = vec![0x06];
        let inputs = vec![encode_cl_swap_single_input(
            token_a(),
            token_b(),
            alloy_primitives::Address::from([0xAB; 20]), // hooks (index 2)
            pool_manager(),
            3_000, // fee (index 4)
            true,
            1,
            1,
        )];
        let steps = dispatch_opcodes(&actions, &inputs, &PANCAKE_INFI_TABLE);

        let registry = EmptyTokenRegistry;
        let from = dummy_addr(0xAA);
        let to = dummy_addr(0xBB);
        let value = DecimalString::from_str("0").unwrap();
        let ctx = build_ctx(&registry, &from, &to, &value);

        let envelopes = build_pancake_infi_swap_envelopes(&ctx, &steps).unwrap();
        let Action::Swap(s) = &envelopes[0].action else {
            panic!("expected Swap");
        };
        // 3000 hundredths of a bp / 100 = 30 bps.
        assert_eq!(s.fee_bps, Some(30));
    }
}
