//! Multi-router CallAdapter — for calls whose single `calldata` contains
//! multiple sub-calls (one outer ABI envelope wrapping N inner operations).
//!
//! The Mapper trait shape (`DecodedCall → ActionEnvelope[]`) can't express
//! this cleanly because the inner sub-calls need their own ABI decode against
//! per-sub-call schemas — that's `Decoder` territory, not `Mapper`. So we
//! drop down to the `CallAdapter` trait, which receives raw calldata and can
//! do whatever internal decoding it needs.
//!
//! Current coverage: Uniswap Universal Router's `execute(commands, inputs[, deadline])`.
//! Future candidates that fit the same "1 calldata → N sub-calls" pattern:
//! - Pancake Universal Router (different opcode table)
//! - Safe `multiSend(bytes transactions)` (packed sub-tx list)
//! - 1inch aggregator multicall
//! - Permit2 batch
//!
//! When adding a new family, prefer extending this file (or splitting it into
//! `multi_router/{uniswap_ur,pancake_ur,safe_multisend,…}.rs` once it gets
//! crowded) rather than introducing a new ad-hoc CallAdapter.

use std::str::FromStr as _;

use abi_resolver::subdecode::opcode_stream::dispatch as dispatch_opcodes;
use abi_resolver::subdecode::protocols::universal_router::{
    uniswap_universal_router_deployments, EXECUTE_DEADLINE_SELECTOR, EXECUTE_SELECTOR,
};
use abi_resolver::subdecode::protocols::v4_router::V4_ROUTER_TABLE;
use abi_resolver::CallMatchKey;
use alloy_dyn_abi::DynSolValue;
use alloy_primitives::U256;
use alloy_sol_types::{sol, SolCall, SolValue};
use policy_engine::action::dex::{SwapAction, SwapMode};
use policy_engine::action::misc::{UnwrapAction, WrapAction};
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountConstraint, AmountKind, AssetKind, AssetRef,
    AssetRefWithAmountConstraint, Category, DecimalString, Validity, ValiditySource,
};

use crate::{AdapterError, CallAdapter, CallAdapterId, CallContext};

// Outer-call decoders for the two Uniswap Universal Router `execute`
// overloads. We use `sol!` inline so this CallAdapter doesn't depend on a
// per-function `Decoder` struct.
sol! {
    #[allow(clippy::too_many_arguments)]
    function execute(bytes commands, bytes[] inputs);
    #[allow(clippy::too_many_arguments)]
    function executeWithDeadline(
        bytes commands,
        bytes[] inputs,
        uint256 deadline,
    );
}

const ADAPTER_ID: &str = "multi-router/uniswap-universal-router";
const WORD_LEN: usize = 32;
const ADDRESS_LEN: usize = 20;
const V3_SWAP_EXACT_IN: u8 = 0x00;
const V3_SWAP_EXACT_OUT: u8 = 0x01;
const PERMIT2_TRANSFER_FROM: u8 = 0x02;
const PERMIT2_PERMIT_BATCH: u8 = 0x03;
const V2_SWAP_EXACT_IN: u8 = 0x08;
const V2_SWAP_EXACT_OUT: u8 = 0x09;
const PERMIT2_PERMIT: u8 = 0x0a;
const WRAP_ETH: u8 = 0x0b;
const UNWRAP_WETH: u8 = 0x0c;
const PERMIT2_TRANSFER_FROM_BATCH: u8 = 0x0d;
const V4_SWAP_OPCODE: u8 = 0x10;
const COMMAND_TYPE_MASK: u8 = 0x7f;

// Inner V4 action opcodes (dispatched against V4_ROUTER_TABLE inside V4_SWAP).
const V4_ACTION_SWAP_EXACT_IN_SINGLE: u8 = 0x06;
const V4_ACTION_SWAP_EXACT_IN: u8 = 0x07;
const V4_ACTION_SWAP_EXACT_OUT_SINGLE: u8 = 0x08;
const V4_ACTION_SWAP_EXACT_OUT: u8 = 0x09;

const WETH_MAINNET: &str = "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";
const ACTION_MSG_SENDER: &str = "0x0000000000000000000000000000000000000001";
const ACTION_ADDRESS_THIS: &str = "0x0000000000000000000000000000000000000002";

#[derive(Debug, Clone, Copy, Default)]
pub struct MultiRouterCallAdapter;

impl MultiRouterCallAdapter {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl CallAdapter for MultiRouterCallAdapter {
    fn id(&self) -> CallAdapterId {
        CallAdapterId::new(ADAPTER_ID)
    }

    fn match_keys(&self) -> Vec<CallMatchKey> {
        let mut out = Vec::new();
        for (chain_id, alloy_addr) in uniswap_universal_router_deployments() {
            let to = policy_address_from_alloy(&alloy_addr);
            for selector in [EXECUTE_SELECTOR, EXECUTE_DEADLINE_SELECTOR] {
                out.push(CallMatchKey {
                    chain_id,
                    to: to.clone(),
                    selector,
                });
            }
        }
        out
    }

    fn build(
        &self,
        ctx: &CallContext<'_>,
        calldata: &[u8],
    ) -> Result<Vec<ActionEnvelope>, AdapterError> {
        let (commands, inputs, validity) = decode_outer_call(calldata)?;
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
                WRAP_ETH => {
                    envelopes.push(decode_wrap_eth(ctx, input)?);
                }
                UNWRAP_WETH => {
                    envelopes.push(decode_unwrap_weth(ctx, input)?);
                }
                V4_SWAP_OPCODE => {
                    envelopes.extend(decode_v4_swap(ctx, input, validity.clone())?);
                }
                // Permit2 family — recognised explicitly so we don't treat
                // them as "unknown opcode" surprises. The permit semantics
                // are gated on the *sign* side by
                // `sign_resolver::adapters::permit2`, which evaluates the
                // typed-data signature the wallet showed the user *before*
                // the swap calldata was even built. Inside UR these commands
                // just replay the same permit (or `transferFrom`) the user
                // already authorised, so we emit no extra envelope here.
                PERMIT2_PERMIT
                | PERMIT2_PERMIT_BATCH
                | PERMIT2_TRANSFER_FROM
                | PERMIT2_TRANSFER_FROM_BATCH => {}
                _ => {}
            }
        }

        Ok(envelopes)
    }
}

// ── V4_SWAP (UR opcode 0x10) ──────────────────────────────────────────────────
// `input = abi.encode(bytes actions, bytes[] params)` per V4Router. The action
// byte string is iterated like UR's own command stream but against
// `V4_ROUTER_TABLE` (which provides per-action JSON-ABI schemas for the inner
// params bytes). Only the 4 swap actions emit `SwapAction` envelopes; settle/
// take/delta-management actions are intentionally skipped today.

sol! {
    #[allow(clippy::too_many_arguments)]
    struct V4SwapInput {
        bytes actions;
        bytes[] params;
    }
}

fn decode_v4_swap(
    ctx: &CallContext<'_>,
    input: &[u8],
    validity: Option<Validity>,
) -> Result<Vec<ActionEnvelope>, AdapterError> {
    let parsed = V4SwapInput::abi_decode_sequence(input, true)
        .map_err(|e| AdapterError::Invalid(format!("V4_SWAP outer decode failed: {e}")))?;
    let actions = parsed.actions.to_vec();
    let params: Vec<Vec<u8>> = parsed.params.iter().map(|b| b.to_vec()).collect();
    let steps = dispatch_opcodes(&actions, &params, &V4_ROUTER_TABLE);

    let mut out = Vec::new();
    for step in &steps {
        let env = match step.opcode {
            V4_ACTION_SWAP_EXACT_IN_SINGLE => v4_swap_exact_in_single(ctx, step, validity.clone())?,
            V4_ACTION_SWAP_EXACT_IN => v4_swap_exact_in_multi(ctx, step, validity.clone())?,
            V4_ACTION_SWAP_EXACT_OUT_SINGLE => {
                v4_swap_exact_out_single(ctx, step, validity.clone())?
            }
            V4_ACTION_SWAP_EXACT_OUT => v4_swap_exact_out_multi(ctx, step, validity.clone())?,
            _ => continue,
        };
        out.push(env);
    }
    Ok(out)
}

/// Extract the top-level `params` struct from a `DecodedStep`. All V4Router
/// swap actions follow the shape `params: (poolKey/currency*, ...)`.
fn v4_params_tuple(
    step: &abi_resolver::subdecode::opcode_stream::DecodedStep,
) -> Result<&[DynSolValue], AdapterError> {
    let args = step.args.as_ref().ok_or_else(|| {
        AdapterError::Invalid(format!("V4 action {} carried no decoded args", step.name))
    })?;
    let first = args
        .first()
        .ok_or_else(|| AdapterError::Invalid(format!("V4 action {} args empty", step.name)))?;
    match &first.value {
        DynSolValue::Tuple(fields) => Ok(fields),
        other => Err(AdapterError::Invalid(format!(
            "V4 action {} expected params tuple, got {other:?}",
            step.name
        ))),
    }
}

fn tuple_address(value: &DynSolValue, field_name: &str) -> Result<Address, AdapterError> {
    match value {
        DynSolValue::Address(addr) => Address::from_str(&format!("0x{}", hex::encode(addr.0)))
            .map_err(|e| AdapterError::Invalid(format!("invalid {field_name} address: {e}"))),
        other => Err(AdapterError::Invalid(format!(
            "expected {field_name} address, got {other:?}"
        ))),
    }
}

fn tuple_bool(value: &DynSolValue, field_name: &str) -> Result<bool, AdapterError> {
    match value {
        DynSolValue::Bool(b) => Ok(*b),
        other => Err(AdapterError::Invalid(format!(
            "expected {field_name} bool, got {other:?}"
        ))),
    }
}

fn tuple_uint(value: &DynSolValue, field_name: &str) -> Result<U256, AdapterError> {
    match value {
        DynSolValue::Uint(u, _) => Ok(*u),
        other => Err(AdapterError::Invalid(format!(
            "expected {field_name} uint, got {other:?}"
        ))),
    }
}

/// `V4Router.swapExactInSingle(params: ExactInputSingleParams)`.
/// params: (poolKey, zeroForOne, amountIn, amountOutMinimum, minHopPriceX36, hookData)
fn v4_swap_exact_in_single(
    ctx: &CallContext<'_>,
    step: &abi_resolver::subdecode::opcode_stream::DecodedStep,
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let fields = v4_params_tuple(step)?;
    let pool_key = fields
        .first()
        .ok_or_else(|| AdapterError::Invalid("V4 ExactInSingle missing poolKey".into()))?;
    let DynSolValue::Tuple(pk) = pool_key else {
        return Err(AdapterError::Invalid("V4 poolKey not a tuple".into()));
    };
    let currency0 = tuple_address(&pk[0], "poolKey.currency0")?;
    let currency1 = tuple_address(&pk[1], "poolKey.currency1")?;
    let zero_for_one = tuple_bool(&fields[1], "zeroForOne")?;
    let amount_in = tuple_uint(&fields[2], "amountIn")?;
    let amount_out_min = tuple_uint(&fields[3], "amountOutMinimum")?;

    let (token_in, token_out) = if zero_for_one {
        (currency0, currency1)
    } else {
        (currency1, currency0)
    };

    Ok(swap_envelope(SwapAction {
        swap_mode: SwapMode::ExactIn,
        input_token: asset_with_amount(
            v4_asset_ref(ctx, &token_in),
            amount_constraint(AmountKind::Exact, decimal(&amount_in.to_string())?),
        ),
        output_token: asset_with_amount(
            v4_asset_ref(ctx, &token_out),
            amount_constraint(AmountKind::Min, decimal(&amount_out_min.to_string())?),
        ),
        recipient: ctx.from.clone(), // V4 doesn't carry recipient in swap params (uses delta + take action)
        validity,
        fee_bps: extract_pool_fee_bps(pk)?,
    }))
}

/// `V4Router.swapExactIn(params: ExactInputParams)`.
/// params: (currencyIn, path[], minHopPriceX36, amountIn, amountOutMinimum)
fn v4_swap_exact_in_multi(
    ctx: &CallContext<'_>,
    step: &abi_resolver::subdecode::opcode_stream::DecodedStep,
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let fields = v4_params_tuple(step)?;
    let currency_in = tuple_address(&fields[0], "currencyIn")?;
    let path_val = &fields[1];
    let DynSolValue::Array(path_items) = path_val else {
        return Err(AdapterError::Invalid("V4 path not an array".into()));
    };
    let last = path_items
        .last()
        .ok_or_else(|| AdapterError::Invalid("V4 path empty".into()))?;
    let DynSolValue::Tuple(last_fields) = last else {
        return Err(AdapterError::Invalid("V4 path entry not tuple".into()));
    };
    let currency_out = tuple_address(&last_fields[0], "path.last.intermediateCurrency")?;

    let amount_in = tuple_uint(&fields[3], "amountIn")?;
    let amount_out_min = tuple_uint(&fields[4], "amountOutMinimum")?;
    let fee_bps = match &last_fields[1] {
        DynSolValue::Uint(u, _) => Some(u32::try_from(*u).unwrap_or(0) / 100),
        _ => None,
    };

    Ok(swap_envelope(SwapAction {
        swap_mode: SwapMode::ExactIn,
        input_token: asset_with_amount(
            v4_asset_ref(ctx, &currency_in),
            amount_constraint(AmountKind::Exact, decimal(&amount_in.to_string())?),
        ),
        output_token: asset_with_amount(
            v4_asset_ref(ctx, &currency_out),
            amount_constraint(AmountKind::Min, decimal(&amount_out_min.to_string())?),
        ),
        recipient: ctx.from.clone(),
        validity,
        fee_bps,
    }))
}

/// `V4Router.swapExactOutSingle(params: ExactOutputSingleParams)`.
/// params: (poolKey, zeroForOne, amountOut, amountInMaximum, minHopPriceX36, hookData)
fn v4_swap_exact_out_single(
    ctx: &CallContext<'_>,
    step: &abi_resolver::subdecode::opcode_stream::DecodedStep,
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let fields = v4_params_tuple(step)?;
    let pool_key = fields
        .first()
        .ok_or_else(|| AdapterError::Invalid("V4 ExactOutSingle missing poolKey".into()))?;
    let DynSolValue::Tuple(pk) = pool_key else {
        return Err(AdapterError::Invalid("V4 poolKey not a tuple".into()));
    };
    let currency0 = tuple_address(&pk[0], "poolKey.currency0")?;
    let currency1 = tuple_address(&pk[1], "poolKey.currency1")?;
    let zero_for_one = tuple_bool(&fields[1], "zeroForOne")?;
    let amount_out = tuple_uint(&fields[2], "amountOut")?;
    let amount_in_max = tuple_uint(&fields[3], "amountInMaximum")?;

    let (token_in, token_out) = if zero_for_one {
        (currency0, currency1)
    } else {
        (currency1, currency0)
    };

    Ok(swap_envelope(SwapAction {
        swap_mode: SwapMode::ExactOut,
        input_token: asset_with_amount(
            v4_asset_ref(ctx, &token_in),
            amount_constraint(AmountKind::Max, decimal(&amount_in_max.to_string())?),
        ),
        output_token: asset_with_amount(
            v4_asset_ref(ctx, &token_out),
            amount_constraint(AmountKind::Exact, decimal(&amount_out.to_string())?),
        ),
        recipient: ctx.from.clone(),
        validity,
        fee_bps: extract_pool_fee_bps(pk)?,
    }))
}

/// `V4Router.swapExactOut(params: ExactOutputParams)`.
/// params: (currencyOut, path[], amountInMaximum, amountOut, [other])
fn v4_swap_exact_out_multi(
    ctx: &CallContext<'_>,
    step: &abi_resolver::subdecode::opcode_stream::DecodedStep,
    validity: Option<Validity>,
) -> Result<ActionEnvelope, AdapterError> {
    let fields = v4_params_tuple(step)?;
    let currency_out = tuple_address(&fields[0], "currencyOut")?;
    let path_val = &fields[1];
    let DynSolValue::Array(path_items) = path_val else {
        return Err(AdapterError::Invalid("V4 path not an array".into()));
    };
    let last = path_items
        .last()
        .ok_or_else(|| AdapterError::Invalid("V4 path empty".into()))?;
    let DynSolValue::Tuple(last_fields) = last else {
        return Err(AdapterError::Invalid("V4 path entry not tuple".into()));
    };
    let currency_in = tuple_address(&last_fields[0], "path.last.intermediateCurrency")?;

    let amount_in_max = tuple_uint(&fields[2], "amountInMaximum")?;
    let amount_out = tuple_uint(&fields[3], "amountOut")?;
    let fee_bps = match &last_fields[1] {
        DynSolValue::Uint(u, _) => Some(u32::try_from(*u).unwrap_or(0) / 100),
        _ => None,
    };

    Ok(swap_envelope(SwapAction {
        swap_mode: SwapMode::ExactOut,
        input_token: asset_with_amount(
            v4_asset_ref(ctx, &currency_in),
            amount_constraint(AmountKind::Max, decimal(&amount_in_max.to_string())?),
        ),
        output_token: asset_with_amount(
            v4_asset_ref(ctx, &currency_out),
            amount_constraint(AmountKind::Exact, decimal(&amount_out.to_string())?),
        ),
        recipient: ctx.from.clone(),
        validity,
        fee_bps,
    }))
}

fn extract_pool_fee_bps(pool_key_fields: &[DynSolValue]) -> Result<Option<u32>, AdapterError> {
    let fee_value = pool_key_fields
        .get(2)
        .ok_or_else(|| AdapterError::Invalid("V4 poolKey missing fee".into()))?;
    match fee_value {
        DynSolValue::Uint(u, _) => Ok(Some(u32::try_from(*u).unwrap_or(0) / 100)),
        _ => Ok(None),
    }
}

/// V4 represents native ETH as `address(0)`. Map that to a `Native` AssetRef;
/// any other address is treated as ERC-20.
fn v4_asset_ref(ctx: &CallContext<'_>, address: &Address) -> AssetRef {
    let lower = address.to_string().to_ascii_lowercase();
    if lower == "0x0000000000000000000000000000000000000000" {
        return AssetRef {
            kind: AssetKind::Native,
            address: None,
            token_id: None,
            symbol: Some("ETH".to_owned()),
            decimals: Some(18),
        };
    }
    asset_ref(ctx, address)
}

/// Decode the outer `execute(bytes,bytes[][,uint256])` ABI envelope via the
/// inline `sol!` macros. Returns `(commands, inputs, optional_validity)` so
/// the opcode dispatcher can iterate without going through a `DecodedCall`.
#[allow(clippy::type_complexity)]
fn decode_outer_call(
    calldata: &[u8],
) -> Result<(Vec<u8>, Vec<Vec<u8>>, Option<Validity>), AdapterError> {
    let selector: [u8; 4] = calldata
        .get(..4)
        .ok_or_else(|| AdapterError::Invalid("UR calldata shorter than selector".into()))?
        .try_into()
        .expect("slice length checked");

    match selector {
        EXECUTE_SELECTOR => {
            let call = executeCall::abi_decode(calldata, true)
                .map_err(|e| AdapterError::Invalid(format!("UR execute ABI decode failed: {e}")))?;
            let inputs: Vec<Vec<u8>> = call.inputs.iter().map(|b| b.to_vec()).collect();
            Ok((call.commands.to_vec(), inputs, None))
        }
        EXECUTE_DEADLINE_SELECTOR => {
            let call = executeWithDeadlineCall::abi_decode(calldata, true).map_err(|e| {
                AdapterError::Invalid(format!("UR executeWithDeadline ABI decode failed: {e}"))
            })?;
            let inputs: Vec<Vec<u8>> = call.inputs.iter().map(|b| b.to_vec()).collect();
            let validity = Some(Validity {
                expires_at: decimal(&call.deadline.to_string())?,
                source: ValiditySource::TxDeadline,
            });
            Ok((call.commands.to_vec(), inputs, validity))
        }
        _ => Err(AdapterError::Invalid(format!(
            "unrecognised UR selector 0x{}",
            hex::encode(selector)
        ))),
    }
}

fn policy_address_from_alloy(addr: &alloy_primitives::Address) -> Address {
    Address::from_str(&format!("0x{}", hex::encode(addr.0)))
        .expect("alloy address always parses as policy Address")
}

/// UR command 0x0b WRAP_ETH — `abi.encode(address recipient, uint256 amountMin)`.
/// Native ETH → WETH mint to recipient. We use the `Misc::Wrap` action.
fn decode_wrap_eth(ctx: &CallContext<'_>, input: &[u8]) -> Result<ActionEnvelope, AdapterError> {
    let recipient = map_recipient(ctx, read_address_word(input, 0)?);
    let amount_min = read_decimal_word(input, 1)?;
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Wrap(WrapAction {
            native_asset: asset_with_amount(
                native_asset(),
                AmountConstraint {
                    kind: AmountKind::Min,
                    value: Some(amount_min.clone()),
                },
            ),
            wrapped_asset: asset_with_amount(
                weth_asset(ctx),
                AmountConstraint {
                    kind: AmountKind::Min,
                    value: Some(amount_min),
                },
            ),
            recipient,
        }),
    })
}

/// UR command 0x0c UNWRAP_WETH — `abi.encode(address recipient, uint256 amountMin)`.
/// WETH burn → native ETH to recipient. We use the `Misc::Unwrap` action.
fn decode_unwrap_weth(ctx: &CallContext<'_>, input: &[u8]) -> Result<ActionEnvelope, AdapterError> {
    let recipient = map_recipient(ctx, read_address_word(input, 0)?);
    let amount_min = read_decimal_word(input, 1)?;
    Ok(ActionEnvelope {
        category: Category::Misc,
        action: Action::Unwrap(UnwrapAction {
            wrapped_asset: asset_with_amount(
                weth_asset(ctx),
                AmountConstraint {
                    kind: AmountKind::Min,
                    value: Some(amount_min.clone()),
                },
            ),
            native_asset: asset_with_amount(
                native_asset(),
                AmountConstraint {
                    kind: AmountKind::Min,
                    value: Some(amount_min),
                },
            ),
            recipient,
        }),
    })
}

fn native_asset() -> AssetRef {
    AssetRef {
        kind: AssetKind::Native,
        address: None,
        token_id: None,
        symbol: Some("ETH".to_owned()),
        decimals: Some(18),
    }
}

fn weth_asset(ctx: &CallContext<'_>) -> AssetRef {
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
        input_token: asset_with_amount(
            asset_ref(ctx, &parsed_path.token_in),
            amount_constraint(AmountKind::Exact, amount_in),
        ),
        output_token: asset_with_amount(
            asset_ref(ctx, &parsed_path.token_out),
            amount_constraint(AmountKind::Min, amount_out_min),
        ),
        recipient,
        validity,
        fee_bps: parsed_path.fee_bps,
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
        input_token: asset_with_amount(
            asset_ref(ctx, &parsed_path.token_out),
            amount_constraint(AmountKind::Max, amount_in_max),
        ),
        output_token: asset_with_amount(
            asset_ref(ctx, &parsed_path.token_in),
            amount_constraint(AmountKind::Exact, amount_out),
        ),
        recipient,
        validity,
        fee_bps: parsed_path.fee_bps,
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
        input_token: asset_with_amount(
            asset_ref(ctx, token_in),
            amount_constraint(AmountKind::Exact, amount_in),
        ),
        output_token: asset_with_amount(
            asset_ref(ctx, token_out),
            amount_constraint(AmountKind::Min, amount_out_min),
        ),
        recipient,
        validity,
        fee_bps: Some(30),
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
        input_token: asset_with_amount(
            asset_ref(ctx, token_in),
            amount_constraint(AmountKind::Max, amount_in_max),
        ),
        output_token: asset_with_amount(
            asset_ref(ctx, token_out),
            amount_constraint(AmountKind::Exact, amount_out),
        ),
        recipient,
        validity,
        fee_bps: Some(30),
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

fn asset_with_amount(asset: AssetRef, amount: AmountConstraint) -> AssetRefWithAmountConstraint {
    AssetRefWithAmountConstraint { asset, amount }
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
    use crate::{CallAdapter as _, MultiRouterCallAdapter};

    #[test]
    fn test_ur_call_adapter_match_keys() {
        assert!(!MultiRouterCallAdapter::new().match_keys().is_empty());
    }
}
