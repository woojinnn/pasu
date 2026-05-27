//! Real on-chain Aerodrome transaction verification harness.
//!
//! Every other Aerodrome test in this crate (`edge_aerodrome_*.rs`) feeds the
//! declarative mapper a **synthetic** `DecodedCall` built by hand from
//! `DecodedValue` literals. None of them exercise the raw-calldata decode step.
//! This harness closes that gap: it takes **real Base mainnet calldata**
//! captured from on-chain Aerodrome transactions and runs it through the full
//! declarative pipeline that production (`declarative_route_request_json`)
//! uses — `decode_with_json_abi` → `DeclarativeMapper::map` → lower → evaluate.
//!
//! Each fixture is a real tx; its hash + block are pinned for provenance
//! (re-fetchable on Basescan / an archive RPC). Calldata is the exact `data`
//! field as broadcast — including any wallet-appended suffix.
//!
//! ## Why not `integration-tests::golden_regression`
//!
//! `request_router::route_request` has no declarative tier — an Aerodrome
//! calldata sent through it returns `RouterError::NoMatch`. This harness drives
//! `DeclarativeMapper` directly, mirroring `declarative_route_request_json`
//! (`policy-engine-wasm/src/declarative_exports.rs:393`).
//!
//! ## The 4 pipeline stages
//!
//! 1. **routing** — `registry/index/by-callkey` routes `(chain,to,selector)`
//!    to the bundle this fixture loads.
//! 2. **decode** — `decode_with_json_abi` decodes raw calldata against the
//!    bundle's `abi_fragment.abi`.
//! 3. **map** — `DeclarativeMapper::map` → `Vec<ActionEnvelope>`.
//! 4. **verdict** — envelope serialises, round-trips through `ActionEnvelope`'s
//!    custom `Deserialize` (the `__engine::invalid_input_json` guard), lowers,
//!    and evaluates.
//!
//! Each fixture is classified by its furthest-reached stage. A `MapFault` /
//! `EvaluateFault` expectation means the harness *documents a real defect* —
//! see `docs/AERODROME_REALTX_VERIFICATION.md`.

use std::path::PathBuf;
use std::str::FromStr as _;

use abi_resolver::bridge::decode_with_json_abi;
use abi_resolver::CallMatchKey;
use mappers::declarative::{types::AdapterFunctionBundle, DeclarativeMapper};
use mappers::mapper::{ChildResolver, MapContext, Mapper, MapperError};
use mappers::EmptyTokenRegistry;
use policy_engine::action::{ActionEnvelope, Address, DecimalString};
use policy_engine::{policy_request_from_envelope, PolicyEngineBuilder, Verdict};

// ───────────────────────────────────────────────────────────────────────────
// Fixture + outcome model
// ───────────────────────────────────────────────────────────────────────────

/// A real on-chain Aerodrome transaction captured from Base mainnet.
struct RealTx {
    label: &'static str,
    /// Base tx hash (provenance).
    tx_hash: &'static str,
    /// Bundle JSON, `include_str!`'d from `registry/manifests/aerodrome/`.
    bundle_json: &'static str,
    chain_id: u64,
    to: &'static str,
    from: &'static str,
    value_wei: &'static str,
    /// Raw `0x`-prefixed calldata, exactly as broadcast.
    calldata: &'static str,
    /// Expected pipeline classification.
    expect: Expect,
}

#[derive(Debug)]
#[allow(dead_code)] // `MapFault` / `EvaluateFault` retained: every fixture
                    // currently lands on `Clean`, but the discriminant is
                    // load-bearing if a future regression flips a fixture.
enum Expect {
    /// All 4 stages pass; `envelopes[0].action.kind()` == the held value.
    Clean(&'static str),
    /// Stage 3 `mapper.map` errors; the message contains the held substring.
    MapFault(&'static str),
    /// Stages 1-3 pass; stage 4 (evaluate / round-trip) errors; message
    /// contains the held substring.
    EvaluateFault(&'static str),
}

/// What actually happened when a `RealTx` ran through the pipeline.
#[derive(Debug)]
enum Report {
    Clean {
        kinds: Vec<String>,
        verdict: String,
        envelope_json: String,
    },
    DecodeFault(String),
    MapFault(String),
    LowerFault(String),
    EvaluateFault(String),
    SetupFault(String),
}

// ───────────────────────────────────────────────────────────────────────────
// Pipeline driver — captures every stage outcome, never panics on tx defects.
// ───────────────────────────────────────────────────────────────────────────

fn registry_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../registry")
}

/// `"0x" + first 4 calldata bytes`.
fn selector_of(calldata: &str) -> String {
    format!("0x{}", &calldata.trim_start_matches("0x")[..8])
}

/// Stage 1 — does the `by-callkey` index route `(chain,to,selector)` to a
/// bundle, and (when it does) is it the bundle this fixture loaded?
fn callkey_status(
    chain_id: u64,
    to: &str,
    calldata: &str,
    expect_bundle_id: Option<&str>,
) -> (bool, Option<String>) {
    let callkey = format!(
        "{}__{}__{}.json",
        chain_id,
        to.to_lowercase(),
        selector_of(calldata)
    );
    let path = registry_root().join("index/by-callkey").join(&callkey);
    match std::fs::read_to_string(&path) {
        Err(_) => (false, None),
        Ok(raw) => {
            let entry: serde_json::Value = serde_json::from_str(&raw).expect("callkey json parses");
            let bundle_id = entry
                .get("bundle_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string();
            if let Some(expected) = expect_bundle_id {
                assert_eq!(
                    bundle_id, expected,
                    "[stage1] callkey {callkey} routes to `{bundle_id}`, harness loaded `{expected}`",
                );
            }
            (true, Some(bundle_id))
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Mock ChildResolver — `multicall_recurse` inner-step dispatch.
//
// Mirrors the production `WasmChildResolver`
// (`policy-engine-wasm/src/declarative_exports.rs:147`) and the integration-
// tests `LocalIndexChildResolver` (`integration-tests/tests/uniswap_real_tx.rs`)
// — both resolve a child `(chain_id, to, selector)` callkey to its bundle and
// run `DeclarativeMapper::map` on the decoded inner calldata.
//
// This harness needs its own copy because `crates/adapters/mappers/tests/` is
// an in-crate integration test and cannot depend on `policy-engine-wasm` or
// the integration-tests crate.
//
// `resolve_child` mirror logic:
//   * compute child callkey from `(chain_id, to, selector)`
//   * look it up in `registry/index/by-callkey/` (single source of truth, same
//     index `callkey_status` queries above)
//   * HIT  → inner `AdapterFunctionBundle` → `DeclarativeMapper` →
//            `decode_with_json_abi(bundle.abi, child_calldata)` → canonicalise
//            `decoded.decoder_id` → `mapper.map(ctx, decoded)` → envelopes
//   * MISS → `Ok(vec![])` — uncovered inner step is recorded as a gap
//            (same semantics as `LocalIndexChildResolver`); the parent's
//            envelope count then reflects the gap. The harness asserts
//            `Expect::Clean(kind)` against `envelopes[0]` so any inner miss
//            shifts the kind off and is detected.
// ───────────────────────────────────────────────────────────────────────────

/// Local-index-backed [`ChildResolver`] for `multicall_recurse` fixtures.
///
/// Mirrors `WasmChildResolver`'s shape, but resolves children against the
/// on-disk `registry/index/by-callkey/` files instead of an in-WASM bridge
/// table.
struct LocalIndexChildResolver;

impl ChildResolver for LocalIndexChildResolver {
    fn resolve_child(
        &self,
        child: &CallMatchKey,
        ctx: &MapContext<'_>,
        child_calldata: &[u8],
    ) -> Result<Vec<ActionEnvelope>, MapperError> {
        let to_str = child.to.to_string();
        let callkey = format!(
            "{}__{}__0x{}.json",
            child.chain_id,
            to_str.to_ascii_lowercase(),
            hex::encode(child.selector),
        );
        let path = registry_root().join("index/by-callkey").join(&callkey);
        let raw = match std::fs::read_to_string(&path) {
            Ok(r) => r,
            // MISS — inner step uncovered. Empty result, not an error: the
            // top-level fixture's `Expect::Clean(kind)` will fail loudly if
            // a key inner step is missing.
            Err(_) => return Ok(vec![]),
        };
        let entry: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
            MapperError::Internal(anyhow::anyhow!("child callkey {callkey} parse failed: {e}"))
        })?;
        let bundle: AdapterFunctionBundle =
            serde_json::from_value(entry.get("bundle").cloned().ok_or_else(|| {
                MapperError::Internal(anyhow::anyhow!(
                    "child callkey {callkey} missing `bundle` field"
                ))
            })?)
            .map_err(|e| {
                MapperError::Internal(anyhow::anyhow!(
                    "child callkey {callkey} bundle parse failed: {e}"
                ))
            })?;

        let mapper = DeclarativeMapper::new(bundle);
        let abi_json = &mapper.bundle().abi_fragment.abi;
        let mut decoded = decode_with_json_abi(abi_json, child_calldata).map_err(|e| {
            MapperError::Internal(anyhow::anyhow!(
                "child decode failed (callkey {callkey}): {e}"
            ))
        })?;
        // `decode_with_json_abi` mints a *static* fallback decoder_id;
        // overwrite with the canonical declarative one — same as
        // `declarative_route_request_json:461` and the parent driver below.
        decoded.decoder_id = mapper.declarative_decoder_id();

        mapper.map(ctx, &decoded)
    }
}

fn run_pipeline(tx: &RealTx) -> (bool, Report) {
    let bundle: AdapterFunctionBundle = match serde_json::from_str(tx.bundle_json) {
        Ok(b) => b,
        Err(e) => return (false, Report::SetupFault(format!("bundle parse: {e}"))),
    };
    // Stage 1
    let (routed, _) = callkey_status(tx.chain_id, tx.to, tx.calldata, Some(&bundle.id));

    let mapper = DeclarativeMapper::new(bundle);

    // Stage 2 — raw calldata → DecodedCall (the step synthetic tests skip).
    let calldata = match hex::decode(tx.calldata.trim_start_matches("0x")) {
        Ok(c) => c,
        Err(e) => return (routed, Report::SetupFault(format!("calldata hex: {e}"))),
    };
    let mut decoded = match decode_with_json_abi(&mapper.bundle().abi_fragment.abi, &calldata) {
        Ok(d) => d,
        Err(e) => return (routed, Report::DecodeFault(e.to_string())),
    };
    // Aerodrome selectors are not in `decode`'s static table → it mints a
    // `fallback/0x..` id. Overwrite with the canonical declarative id, exactly
    // as `declarative_route_request_json:460` does.
    decoded.decoder_id = mapper.declarative_decoder_id();

    // Stage 3 — DecodedCall → Vec<ActionEnvelope>.
    let ctx = Ctx::from_tx(tx);
    let envelopes = match mapper.map(&ctx.map_ctx(), &decoded) {
        Ok(e) => e,
        Err(e) => return (routed, Report::MapFault(e.to_string())),
    };
    if envelopes.is_empty() {
        return (
            routed,
            Report::MapFault("mapper produced zero envelopes".into()),
        );
    }
    let kinds: Vec<String> = envelopes
        .iter()
        .map(|e| e.action.kind().to_string())
        .collect();

    // Stage 4 — serialize → deserialize round-trip → lower → evaluate.
    let env0 = &envelopes[0];
    let json = serde_json::to_string(env0).expect("envelope serialises");
    let roundtripped: ActionEnvelope = match serde_json::from_str(&json) {
        Ok(r) => r,
        Err(e) => {
            return (
                routed,
                Report::EvaluateFault(format!("invalid_input_json: {e}")),
            )
        }
    };
    let from = Address::from_str(tx.from).expect("from address");
    let to = Address::from_str(tx.to).expect("to address");
    let value = DecimalString::from_str(tx.value_wei).expect("value");
    let request = match policy_request_from_envelope(
        &roundtripped,
        &from,
        &to,
        &value,
        tx.chain_id,
        1_700_000_000,
    ) {
        Some(r) => r,
        None => return (routed, Report::LowerFault("envelope did not lower".into())),
    };
    let engine = PolicyEngineBuilder::new()
        .build()
        .expect("policy engine builds");
    match engine.evaluate(
        &request.principal,
        &request.action,
        &request.resource,
        &request.entities,
        &request.context,
    ) {
        Ok(verdict) => (
            routed,
            Report::Clean {
                kinds,
                verdict: format!("{verdict:?}"),
                envelope_json: json,
            },
        ),
        Err(e) => (routed, Report::EvaluateFault(format!("{e:?}"))),
    }
}

fn report_matches(report: &Report, expect: &Expect) -> bool {
    match (report, expect) {
        (Report::Clean { kinds, .. }, Expect::Clean(kind)) => {
            kinds.first().map(String::as_str) == Some(*kind)
        }
        (Report::MapFault(msg), Expect::MapFault(needle)) => msg.contains(needle),
        (Report::EvaluateFault(msg), Expect::EvaluateFault(needle)) => msg.contains(needle),
        _ => false,
    }
}

impl Report {
    /// One-line label for the coverage log (omits the bulky envelope JSON).
    fn tag(&self) -> String {
        match self {
            Report::Clean { kinds, verdict, .. } => format!("Clean {kinds:?} verdict={verdict}"),
            Report::DecodeFault(m) => format!("DecodeFault({m})"),
            Report::MapFault(m) => format!("MapFault({m})"),
            Report::LowerFault(m) => format!("LowerFault({m})"),
            Report::EvaluateFault(m) => format!("EvaluateFault({m})"),
            Report::SetupFault(m) => format!("SetupFault({m})"),
        }
    }
}

/// **Stage 3 deep check** — permission-surface field assertions per Clean
/// fixture, `(JSON pointer into the envelope, expected value)`.
///
/// Expected values are an **independent `cast calldata-decode`** of the same
/// tx — the harness's own `decode_with_json_abi` must not be its own ground
/// truth. Addresses compared case-insensitively. Empty slice ⇒ the fixture is
/// not Clean (faults earlier) so there is no envelope to check.
fn field_checks_for(label: &str) -> &'static [(&'static str, &'static str)] {
    match label {
        "v2/swapExactTokensForTokens" => &[
            (
                "/fields/inputToken/asset/address",
                "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            ),
            ("/fields/inputToken/amount/value", "2015000"),
            (
                "/fields/outputToken/asset/address",
                "0xacfe6019ed1a7dc6f7b508c02d1b04ec88cc21bf",
            ),
            ("/fields/outputToken/amount/value", "88721624932503160"),
            (
                "/fields/recipient",
                "0x0000000000000000000000000000000000000000",
            ),
        ],
        "v2/swapExactTokensForETH (FOT)" => &[
            (
                "/fields/inputToken/asset/address",
                "0x8c0d3adcf8ce094e1ae437557ec90a6374dc9bdd",
            ),
            ("/fields/inputToken/amount/value", "7220426081090"),
            ("/fields/outputToken/asset/kind", "native"),
            ("/fields/outputToken/amount/value", "37792780447103985"),
            (
                "/fields/recipient",
                "0x459bf05de05266ec050b684d36af4ed57c2c2449",
            ),
        ],
        "v2/swapExactETHForTokens (FOT)" => &[
            ("/fields/inputToken/asset/kind", "native"),
            ("/fields/inputToken/amount/value", "28326770512840067"),
            (
                "/fields/outputToken/asset/address",
                "0x8c0d3adcf8ce094e1ae437557ec90a6374dc9bdd",
            ),
            ("/fields/outputToken/amount/value", "5270911039196"),
            (
                "/fields/recipient",
                "0xfa1c5e3d316dbe5c478766984bf0b6d5d34a333d",
            ),
        ],
        "v2/swapExactTokensForTokens (FOT)" => &[
            (
                "/fields/inputToken/asset/address",
                "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            ),
            ("/fields/inputToken/amount/value", "30000000"),
            (
                "/fields/outputToken/asset/address",
                "0xa1832f7f4e534ae557f9b5ab76de54b1873e498b",
            ),
            ("/fields/outputToken/amount/value", "3479713988124019916998"),
            (
                "/fields/recipient",
                "0xae6bbb0ce3329e7e50d028a4c14db645e666688e",
            ),
        ],
        "v2/addLiquidity (wallet-suffixed)" => &[
            (
                "/fields/inputTokens/0/asset/address",
                "0x940181a94a35a4569e4529a3cdfb74e38fd98631",
            ),
            ("/fields/inputTokens/0/amount/value", "24211811029665176736"),
            (
                "/fields/inputTokens/1/asset/address",
                "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            ),
            ("/fields/inputTokens/1/amount/value", "10258326"),
            (
                "/fields/recipient",
                "0xcbfeb33301272fab8e4e2f120e87115296534845",
            ),
            // pool.address = 0x0 sentinel — Aerodrome V2 pool is CREATE2-derived,
            // not in calldata (engagement Q0 / known limitation).
            (
                "/fields/pool/address",
                "0x0000000000000000000000000000000000000000",
            ),
        ],
        "v2/addLiquidityETH (wallet-suffixed)" => &[
            (
                "/fields/inputTokens/0/asset/address",
                "0xacfe6019ed1a7dc6f7b508c02d1b04ec88cc21bf",
            ),
            (
                "/fields/inputTokens/0/amount/value",
                "1992116165708411496681",
            ),
            ("/fields/inputTokens/1/asset/kind", "native"),
            ("/fields/inputTokens/1/amount/value", "15785375570670469818"),
            (
                "/fields/recipient",
                "0x5c98a04f663f32bb5e0778b1b9d8cb71cbdce3cb",
            ),
        ],
        "v2/removeLiquidity" => &[
            ("/fields/inputLp/amount/value", "145460372358636154"),
            (
                "/fields/outputTokens/0/asset/address",
                "0x74ccbe53f77b08632ce0cb91d3a545bf6b8e0979",
            ),
            (
                "/fields/outputTokens/0/amount/value",
                "1337520511386154431477344",
            ),
            (
                "/fields/outputTokens/1/asset/address",
                "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            ),
            ("/fields/outputTokens/1/amount/value", "15929416224"),
            (
                "/fields/recipient",
                "0x28aa4f9ffe21365473b64c161b566c3cdead0108",
            ),
        ],
        // UR opcode-stream — input/output token + recipient (the inner V3 swap
        // input). Each address independently confirmed present in the calldata.
        "universal-router/execute" => &[
            (
                "/fields/inputToken/asset/address",
                "0x50d2280441372486beecdd328c1854743ebacb07",
            ),
            (
                "/fields/outputToken/asset/address",
                "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            ),
            (
                "/fields/recipient",
                "0x07a0c4c00323f6a594ab2d501ae013a3dae4a33e",
            ),
        ],
        // T2-1 — UR `0x24856bc3` no-deadline. Inner opcode `0x01`
        // = V3_SWAP_EXACT_OUT (path is reversed: input = path last_token,
        // output = path first_token). Ground truth from `cast calldata-decode
        // execute(bytes,bytes[])` → inner `(address,uint256,uint256,bytes,bool)`.
        "universal-router/execute(bytes,bytes[]) — no-deadline" => &[
            (
                "/fields/inputToken/asset/address",
                "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            ),
            (
                "/fields/outputToken/asset/address",
                "0x9126236476efba9ad8ab77855c60eb5bf37586eb",
            ),
            (
                "/fields/recipient",
                "0x4eff8063e497b5ef4214a614e5248a5e10c8f4f2",
            ),
        ],
        "voter/vote" => &[
            ("/fields/tokenId", "118882"),
            (
                "/fields/pools/0",
                "0xef7e596aef9e4c6301b4d1f1e88f8ffe8c306222",
            ),
            ("/fields/weights/0", "100000000000000000000"),
        ],
        "voting-escrow/createLock" => &[
            // Stage 0+ wrapper refactor: lock_create.asset is now
            // AssetRefWithAmountConstraint, so amount.value lives at
            // /fields/asset/amount/value.
            ("/fields/asset/amount/value", "14400000000000000"),
            ("/fields/lockDurationSec", "46656000"),
            (
                "/fields/recipient",
                "0x0fe8a3ff06996db01ed5add020453de99548edce",
            ),
        ],
        "voting-escrow/increaseAmount" => &[
            ("/fields/tokenId", "117875"),
            ("/fields/additionalAmount/value", "99805892608599407760"),
        ],
        "voting-escrow/merge" => &[
            ("/fields/fromTokenId", "119945"),
            ("/fields/toTokenId", "119946"),
        ],
        "gauge/deposit (wallet-suffixed)" => &[
            // Stage 0+ wrapper refactor: lp_stake.lpToken is now
            // AssetRefWithAmountConstraint, so amount.value lives at
            // /fields/lpToken/amount/value and the asset kind at
            // /fields/lpToken/asset/kind.
            ("/fields/lpToken/amount/value", "226720148733"),
            (
                "/fields/recipient",
                "0xa8b9b8b02f1caf1b7a9825eb7c568e58eea8eca0",
            ),
            (
                "/fields/gauge",
                "0x519bbd1dd8c6a94c46080e24f316c14ee758c025",
            ),
            // Phase D B-3 fix: lpToken.asset.kind = "unknown" (was `erc20` +
            // misleading `$.tx.to` placeholder address). The staked LP token
            // is un-derivable from `deposit(uint256)` calldata; with
            // kind:unknown the address slot is empty.
            ("/fields/lpToken/asset/kind", "unknown"),
        ],
        // ─── Phase A B-1 — Slipstream router 4 fixtures (Clean after fix) ─────
        // Ground truth = `cast calldata-decode` of the same raw calldata, run
        // independently of the harness's own decoder.
        "slipstream/exactInputSingle" => &[
            (
                "/fields/inputToken/asset/address",
                "0x370923d39f139c64813f173a1bf0b4f9ba36a24f",
            ),
            (
                "/fields/inputToken/amount/value",
                "243470898314649617532268",
            ),
            (
                "/fields/outputToken/asset/address",
                "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            ),
            ("/fields/outputToken/amount/value", "166619600"),
            (
                "/fields/recipient",
                "0xbbf09c739fdfc0408ba80fb6e6dcb72a1d4a1bfe",
            ),
        ],
        "slipstream/exactInput" => &[
            // path = 0x1bc0c42215582d5a085795f4badbac3ff36d1bcb + 0000c8 +
            //        0x4200000000000000000000000000000000000006 + 000064 +
            //        0x833589fcd6edb6e08f4c7c32d4f71b54bda02913 (3 hops)
            // exact_in: first_token = inputToken, last_token = outputToken
            (
                "/fields/inputToken/asset/address",
                "0x1bc0c42215582d5a085795f4badbac3ff36d1bcb",
            ),
            ("/fields/inputToken/amount/value", "94640000000000000000"),
            (
                "/fields/outputToken/asset/address",
                "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            ),
            ("/fields/outputToken/amount/value", "2221450634"),
            (
                "/fields/recipient",
                "0xa73072adc6c34859426fcc29bc6ca2cac07c93c3",
            ),
        ],
        "slipstream/exactOutputSingle" => &[
            // exact_out: input.amount = amountInMaximum, output.amount = amountOut
            (
                "/fields/inputToken/asset/address",
                "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            ),
            ("/fields/inputToken/amount/value", "86508424"),
            (
                "/fields/outputToken/asset/address",
                "0x4200000000000000000000000000000000000006",
            ),
            ("/fields/outputToken/amount/value", "40000000000000000"),
            (
                "/fields/recipient",
                "0x3bf66b5ba807ec0e8faa33ac15c283e05dfad379",
            ),
        ],
        "slipstream/exactOutput" => &[
            // path = 0x4c87da04887a1f9f21f777e3a8dd55c3c9f84701 + 0000c8 +
            //        0x4200000000000000000000000000000000000006 + 000064 +
            //        0x833589fcd6edb6e08f4c7c32d4f71b54bda02913
            // exact_out: inputToken = path last_token (the asset paid in),
            //            outputToken = path first_token (the asset received).
            (
                "/fields/inputToken/asset/address",
                "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            ),
            ("/fields/inputToken/amount/value", "50075000"),
            (
                "/fields/outputToken/asset/address",
                "0x4c87da04887a1f9f21f777e3a8dd55c3c9f84701",
            ),
            (
                "/fields/outputToken/amount/value",
                "205753532766905370050086",
            ),
            (
                "/fields/recipient",
                "0x8678f58ac6c4748b5289d0db70e627eef395dead",
            ),
        ],
        // ─── Phase A B-1 — Slipstream NPM 4 fixtures (Clean after fix) ────────
        //
        // V2 manifest semantics: `inputTokens[i].amount` carries `kind=min`,
        // so `amount.value` reflects `amount{0,1}Min` (slippage floor) — the
        // user's intent token amount (`amount{0,1}Desired`) is *not* the
        // permission surface. In this real tx Min = 0 (the wallet did not
        // enforce a minimum), matching the Uniswap NFPM precedent.
        "slipstream-npm/mint" => &[
            (
                "/fields/inputTokens/0/asset/address",
                "0x1111111111166b7fe7bd91427724b487980afc69",
            ),
            ("/fields/inputTokens/0/amount/value", "0"),
            (
                "/fields/inputTokens/1/asset/address",
                "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913",
            ),
            ("/fields/inputTokens/1/amount/value", "0"),
            (
                "/fields/recipient",
                "0x5ea80c1699bb786ade0e58d0b0c40ff2a6974bf0",
            ),
            // pool.address = 0x0 sentinel — slipstream pool not derivable from
            // calldata (analogous to V2 addLiquidity).
            (
                "/fields/pool/address",
                "0x0000000000000000000000000000000000000000",
            ),
        ],
        "slipstream-npm/increaseLiquidity" => &[
            // nft.address = $.tx.to = Aerodrome Slipstream NPM on Base
            (
                "/fields/nft/address",
                "0x827922686190790b37229fd06084350e74485b72",
            ),
            ("/fields/nft/tokenId", "28450522"),
            // amount.value = amount{0,1}Min (slippage floor); see mint above.
            ("/fields/inputTokens/0/amount/value", "0"),
            ("/fields/inputTokens/1/amount/value", "0"),
        ],
        "slipstream-npm/decreaseLiquidity" => &[
            (
                "/fields/nft/address",
                "0x827922686190790b37229fd06084350e74485b72",
            ),
            ("/fields/nft/tokenId", "71112746"),
            ("/fields/liquidityDelta/value", "56059427952970733145572"),
        ],
        "slipstream-npm/collect" => &[
            // V2 bundle: nft.kind = "erc721" (was "unknown" in v1 because
            // AssetRef.tokenId was missing; v2 schema carries tokenId
            // alongside kind+address, so erc721 round-trips correctly).
            ("/fields/nft/kind", "erc721"),
            ("/fields/tokenId", "71058132"),
            ("/fields/from", "0xb11727bf13d29e20680f3fa74d15a8d76b33b430"),
            (
                "/fields/recipient",
                "0xb11727bf13d29e20680f3fa74d15a8d76b33b430",
            ),
        ],
        // ─── Phase E — multicall fixtures (mock ChildResolver wired) ───────────
        //
        // `Report::Clean.envelope_json` carries `envelopes[0]` only, so field
        // checks below cover the *first* inner step. The `kinds` log in
        // `real_tx_classification` asserts the full ordered sequence of inner
        // envelopes (e.g. `["decrease_liquidity", "claim_rewards"]`), giving
        // the second envelope a second line of defence.
        //
        // Ground truth: independent on-chain calldata decode of the inner
        // `bytes` element (multicall arg 0, ABI `bytes[]`):
        //   slipstream/multicall — inner = exactInputSingle(...) selector
        //     0xa026383e. Decoded fields below.
        //   slipstream-npm/multicall — inner[0] = decreaseLiquidity(...)
        //     selector 0x0c49ccbe; inner[1] = collect(...) selector 0xfc6f7865.
        //     Field checks here cover inner[0] (the decrease_liquidity envelope).
        "slipstream/multicall" => &[
            // exactInputSingle inner — tokenIn = 0x4200…0006 (WETH on Base)
            (
                "/fields/inputToken/asset/address",
                "0x4200000000000000000000000000000000000006",
            ),
            // amountIn = 0xd8bac35e547c52 = 61003943233485906 (= tx.value_wei —
            // the multicall forwards ETH that the router wraps to WETH first).
            ("/fields/inputToken/amount/value", "61003943233485906"),
            (
                "/fields/outputToken/asset/address",
                "0x4da9a0f397db1397902070f93a4d6ddbc0e0e6e8",
            ),
            // amountOutMinimum = 0x115473824344e0136 = 19980000000000000310
            ("/fields/outputToken/amount/value", "19980000000000000310"),
            // recipient = $.args.recipient = caller
            (
                "/fields/recipient",
                "0x95f4665ccb2f1bf3d7c42f85ef4a88b9a68a1b59",
            ),
        ],
        "slipstream-npm/multicall" => &[
            // inner[0] = decreaseLiquidity(tokenId=0xc2d05e=12767326,
            //   liquidity=0x47d08db8887580b=323424487221975051, ...)
            // bundle nft.address = $.tx.to = Aerodrome Slipstream NFPM @ Base
            (
                "/fields/nft/address",
                "0x827922686190790b37229fd06084350e74485b72",
            ),
            ("/fields/nft/tokenId", "12767326"),
            ("/fields/liquidityDelta/value", "323424487221975051"),
        ],
        // Phase B / B-2 — these 3 fixtures flipped from
        // `EvaluateFault("sourceLabel")` to `Clean("claim_rewards")` once
        // `sourceLabel?: String` landed in the cedarschema. Field-checks lock
        // the per-bundle source-address literal + per-tx caller as
        // independent ground truth (manifests + Etherscan `cast tx`).
        "voter/claimBribes" => &[
            // bundle source.address literal = Aerodrome Voter
            (
                "/fields/source/address",
                "0x16613524e02ad97edfef371bc883f2f5d6c480a5",
            ),
            ("/fields/source/label", "Aerodrome Voter (Bribes)"),
            // tokenId = $.args.tokenId = 0x14169 = 82281
            ("/fields/tokenId", "82281"),
            ("/fields/from", "0x9a589729132a053e6bed0fbfe97a75cc7094fd07"),
            (
                "/fields/recipient",
                "0x9a589729132a053e6bed0fbfe97a75cc7094fd07",
            ),
        ],
        "rewards-distributor/claim" => &[
            // bundle source.address literal = RewardsDistributor
            (
                "/fields/source/address",
                "0x227f65131a261548b057215bb1d5ab2997964c7d",
            ),
            ("/fields/source/label", "Aerodrome veAERO Rewards"),
            // tokenId = $.args.tokenId = 0x988b = 39051
            ("/fields/tokenId", "39051"),
            ("/fields/from", "0x4227185aac699ffb5d18707ebc46ee5568370151"),
            (
                "/fields/recipient",
                "0x4227185aac699ffb5d18707ebc46ee5568370151",
            ),
            // RewardsDistributor reward token = AERO (literal in bundle).
            (
                "/fields/rewardTokens/0/address",
                "0x940181a94a35a4569e4529a3cdfb74e38fd98631",
            ),
        ],
        "gauge/getReward (wallet-suffixed)" => &[
            // source.address = $.tx.to (the gauge contract itself)
            (
                "/fields/source/address",
                "0x4f09bab2f0e15e2a078a227fe1537665f55b8360",
            ),
            ("/fields/source/label", "Aerodrome V2 Gauge"),
            // from / recipient = $.args.account (the wallet-suffix's
            // address arg, which matches `$.tx.from` in this fixture)
            ("/fields/from", "0xaecf89718604b2edb5b7fbe6203448755b7d9525"),
            (
                "/fields/recipient",
                "0xaecf89718604b2edb5b7fbe6203448755b7d9525",
            ),
            // Bundle reward token literal = AERO
            (
                "/fields/rewardTokens/0/address",
                "0x940181a94a35a4569e4529a3cdfb74e38fd98631",
            ),
        ],
        _ => &[],
    }
}

/// swap recipient self-guard — mirrors `edge_aerodrome_v2.rs` T12-T15.
const POLICY_SWAP_RECIPIENT_GUARD: &str = r#"@id("user/swap-recipient-self")
@severity("deny")
@reason("swap output recipient differs from the signing wallet")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context.recipient != principal.address
};
"#;

/// Re-run a fixture through the pipeline and evaluate `envelope[0]` against
/// `policy` — the Stage 4 policy-gating check (proves the verdict tracks a
/// real decoded field, not just "evaluate did not fault").
fn evaluate_with_policy(tx: &RealTx, policy: &str) -> Verdict {
    let bundle: AdapterFunctionBundle =
        serde_json::from_str(tx.bundle_json).expect("bundle parses");
    let mapper = DeclarativeMapper::new(bundle);
    let calldata = hex::decode(tx.calldata.trim_start_matches("0x")).expect("calldata hex");
    let mut decoded =
        decode_with_json_abi(&mapper.bundle().abi_fragment.abi, &calldata).expect("decode");
    decoded.decoder_id = mapper.declarative_decoder_id();
    let ctx = Ctx::from_tx(tx);
    let envelopes = mapper.map(&ctx.map_ctx(), &decoded).expect("map");
    let json = serde_json::to_string(&envelopes[0]).expect("serialise");
    let roundtripped: ActionEnvelope = serde_json::from_str(&json).expect("roundtrip");
    let from = Address::from_str(tx.from).expect("from");
    let to = Address::from_str(tx.to).expect("to");
    let value = DecimalString::from_str(tx.value_wei).expect("value");
    let request = policy_request_from_envelope(
        &roundtripped,
        &from,
        &to,
        &value,
        tx.chain_id,
        1_700_000_000,
    )
    .expect("lower");
    PolicyEngineBuilder::new()
        .add_text(policy)
        .build()
        .expect("engine builds")
        .evaluate(
            &request.principal,
            &request.action,
            &request.resource,
            &request.entities,
            &request.context,
        )
        .expect("evaluate")
}

// ───────────────────────────────────────────────────────────────────────────
// MapContext helper — per-tx from/to/value, Base chain (8453).
// ───────────────────────────────────────────────────────────────────────────

struct Ctx {
    registry: EmptyTokenRegistry,
    resolver: LocalIndexChildResolver,
    from: Address,
    to: Address,
    value: DecimalString,
}

impl Ctx {
    fn from_tx(tx: &RealTx) -> Self {
        Self {
            registry: EmptyTokenRegistry,
            resolver: LocalIndexChildResolver,
            from: Address::from_str(tx.from).expect("from address"),
            to: Address::from_str(tx.to).expect("to address"),
            value: DecimalString::from_str(tx.value_wei).expect("value"),
        }
    }

    /// Build a `MapContext` wired with the local-index `ChildResolver`.
    ///
    /// Wiring the resolver unconditionally mirrors `declarative_route_request_
    /// json` (which sets `Some(&WasmChildResolver)` even for `single_emit`
    /// bundles that ignore it). `single_emit` / `opcode_stream_dispatch` /
    /// `enum_tagged_dispatch` bundles never touch `ctx.resolver`; only
    /// `multicall_recurse` consults it.
    fn map_ctx(&self) -> MapContext<'_> {
        MapContext {
            chain_id: 8453,
            from: &self.from,
            to: &self.to,
            value_wei: &self.value,
            block_timestamp: Some(1_700_000_000),
            token_registry: &self.registry,
            parent_calldata: None,
            depth: 0,
            resolver: Some(&self.resolver),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Bundle fixtures — include_str! from registry/manifests/aerodrome/.
// ───────────────────────────────────────────────────────────────────────────

macro_rules! bundle {
    ($path:literal) => {
        include_str!(concat!("../../../../registry/manifests/aerodrome/", $path))
    };
}

const B_V2_SWAP_TT: &str = bundle!("router-v2/swapExactTokensForTokens@1.0.0.json");
const B_V2_SWAP_TE_FOT: &str =
    bundle!("router-v2/swapExactTokensForETHSupportingFeeOnTransferTokens@1.0.0.json");
const B_V2_SWAP_ET_FOT: &str =
    bundle!("router-v2/swapExactETHForTokensSupportingFeeOnTransferTokens@1.0.0.json");
const B_V2_SWAP_TT_FOT: &str =
    bundle!("router-v2/swapExactTokensForTokensSupportingFeeOnTransferTokens@1.0.0.json");
const B_V2_ADD_LIQ: &str = bundle!("router-v2/addLiquidity@1.0.0.json");
const B_V2_ADD_LIQ_ETH: &str = bundle!("router-v2/addLiquidityETH@1.0.0.json");
const B_V2_REMOVE_LIQ: &str = bundle!("router-v2/removeLiquidity@1.0.0.json");
const B_SLIP_EXACT_IN_SINGLE: &str = bundle!("slipstream-swap-router/exactInputSingle@1.0.0.json");
const B_SLIP_EXACT_IN: &str = bundle!("slipstream-swap-router/exactInput@1.0.0.json");
const B_SLIP_EXACT_OUT_SINGLE: &str =
    bundle!("slipstream-swap-router/exactOutputSingle@1.0.0.json");
const B_SLIP_EXACT_OUT: &str = bundle!("slipstream-swap-router/exactOutput@1.0.0.json");
const B_SLIP_MULTICALL: &str = bundle!("slipstream-swap-router/multicall@1.0.0.json");
const B_NPM_MINT: &str = bundle!("slipstream-nfpm/mint@1.0.0.json");
const B_NPM_INCREASE: &str = bundle!("slipstream-nfpm/increaseLiquidity@1.0.0.json");
const B_NPM_DECREASE: &str = bundle!("slipstream-nfpm/decreaseLiquidity@1.0.0.json");
const B_NPM_COLLECT: &str = bundle!("slipstream-nfpm/collect@1.0.0.json");
const B_NPM_MULTICALL: &str = bundle!("slipstream-nfpm/multicall@1.0.0.json");
const B_UR_EXECUTE: &str = bundle!("universal-router/execute@1.0.0.json");
const B_UR_EXECUTE_NO_DEADLINE: &str = bundle!("universal-router/execute-no-deadline@1.0.0.json");
const B_VOTER_VOTE: &str = bundle!("voter/vote@1.0.0.json");
const B_VOTER_CLAIM_BRIBES: &str = bundle!("voter/claimBribes@1.0.0.json");
const B_VE_CREATE_LOCK: &str = bundle!("voting-escrow/createLock@1.0.0.json");
const B_VE_INCREASE_AMOUNT: &str = bundle!("voting-escrow/increaseAmount@1.0.0.json");
const B_VE_MERGE: &str = bundle!("voting-escrow/merge@1.0.0.json");
const B_REWARDS_CLAIM: &str = bundle!("rewards-distributor/claim@1.0.0.json");
const B_GAUGE_GET_REWARD: &str = bundle!("gauge/getReward@1.0.0.json");
const B_GAUGE_DEPOSIT: &str = bundle!("gauge/deposit@1.0.0.json");

// ───────────────────────────────────────────────────────────────────────────
// Real-tx fixtures (Base mainnet 8453). Calldata = exact on-chain `data`.
// ───────────────────────────────────────────────────────────────────────────

const FIXTURES: &[RealTx] = &[
    // ─── V2 Router ──────────────────────────────────────────────────────────
    RealTx {
        label: "v2/swapExactTokensForTokens",
        tx_hash: "0xdabefedf4d88ca32d214ebd5231913f3c267a549de915b20cb412ff76d1190cf",
        bundle_json: B_V2_SWAP_TT, chain_id: 8453,
        to: "0xcf77a3ba9a5ca399b7c97c74d54e5b1beb874e43",
        from: "0xe3157fc487c8d17bc4e7dc09c8186df657f682e7", value_wei: "0",
        calldata: "0xcac88ea900000000000000000000000000000000000000000000000000000000001ebf18000000000000000000000000000000000000000000000000013b33d909ff4e7800000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006a0ecb100000000000000000000000000000000000000000000000000000000000000002000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda0291300000000000000000000000042000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000420dd381b31aef6683db6b902084cb0ffece40da0000000000000000000000004200000000000000000000000000000000000006000000000000000000000000acfe6019ed1a7dc6f7b508c02d1b04ec88cc21bf0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000420dd381b31aef6683db6b902084cb0ffece40da",
        expect: Expect::Clean("swap"),
    },
    RealTx {
        label: "v2/swapExactTokensForETH (FOT)",
        tx_hash: "0x9b0ed7487de580f96a852937dd3182e318525a578452b093f0829b66e4df6188",
        bundle_json: B_V2_SWAP_TE_FOT, chain_id: 8453,
        to: "0xcf77a3ba9a5ca399b7c97c74d54e5b1beb874e43",
        from: "0x459bf05de05266ec050b684d36af4ed57c2c2449", value_wei: "0",
        calldata: "0x12bc3aca0000000000000000000000000000000000000000000000000000069122ee834200000000000000000000000000000000000000000000000000864455659fbbf100000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000459bf05de05266ec050b684d36af4ed57c2c2449000000000000000000000000000000000000000000000000000000006a10157200000000000000000000000000000000000000000000000000000000000000010000000000000000000000008c0d3adcf8ce094e1ae437557ec90a6374dc9bdd00000000000000000000000042000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000420dd381b31aef6683db6b902084cb0ffece40da",
        expect: Expect::Clean("swap"),
    },
    RealTx {
        label: "v2/swapExactETHForTokens (FOT)",
        tx_hash: "0xf9011551355528673073270ce3fbd792b723bad4ba0b42eb21da840dcd558075",
        bundle_json: B_V2_SWAP_ET_FOT, chain_id: 8453,
        to: "0xcf77a3ba9a5ca399b7c97c74d54e5b1beb874e43",
        from: "0xfa1c5e3d316dbe5c478766984bf0b6d5d34a333d", value_wei: "28326770512840067",
        calldata: "0x3da5acba000000000000000000000000000000000000000000000000000004cb3ac7b6dc0000000000000000000000000000000000000000000000000000000000000080000000000000000000000000fa1c5e3d316dbe5c478766984bf0b6d5d34a333d000000000000000000000000000000000000000000000000000000006a101573000000000000000000000000000000000000000000000000000000000000000100000000000000000000000042000000000000000000000000000000000000060000000000000000000000008c0d3adcf8ce094e1ae437557ec90a6374dc9bdd0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000420dd381b31aef6683db6b902084cb0ffece40da",
        expect: Expect::Clean("swap"),
    },
    RealTx {
        label: "v2/swapExactTokensForTokens (FOT)",
        tx_hash: "0x14a4f36c38108544c19a7b45a4b8f6a21cc2e80e27c8692ea85fb6de2d16b931",
        bundle_json: B_V2_SWAP_TT_FOT, chain_id: 8453,
        to: "0xcf77a3ba9a5ca399b7c97c74d54e5b1beb874e43",
        from: "0xae6bbb0ce3329e7e50d028a4c14db645e666688e", value_wei: "0",
        calldata: "0x88cd821e0000000000000000000000000000000000000000000000000000000001c9c3800000000000000000000000000000000000000000000000bca2bb7be24a0a00c600000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000ae6bbb0ce3329e7e50d028a4c14db645e666688e000000000000000000000000000000000000000000000000000000006a0ea62c0000000000000000000000000000000000000000000000000000000000000002000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda0291300000000000000000000000042000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000420dd381b31aef6683db6b902084cb0ffece40da0000000000000000000000004200000000000000000000000000000000000006000000000000000000000000a1832f7f4e534ae557f9b5ab76de54b1873e498b0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000420dd381b31aef6683db6b902084cb0ffece40da",
        expect: Expect::Clean("swap"),
    },
    RealTx {
        label: "v2/addLiquidity (wallet-suffixed)",
        tx_hash: "0x38808295718446183dfeb72c0005cc44f6c618cc41a00a67aa30538422519e34",
        bundle_json: B_V2_ADD_LIQ, chain_id: 8453,
        to: "0xcf77a3ba9a5ca399b7c97c74d54e5b1beb874e43",
        from: "0xcbfeb33301272fab8e4e2f120e87115296534845", value_wei: "0",
        calldata: "0x5a47ddc3000000000000000000000000940181a94a35a4569e4529a3cdfb74e38fd98631000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda0291300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000015001a1da1013dca000000000000000000000000000000000000000000000000000000000009c879600000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000cbfeb33301272fab8e4e2f120e87115296534845000000000000000000000000000000000000000000000000000000006a0eca808779ce964b87d3f89643854abe0262635f7239717a66386f700b0080218021802180218021802180218021",
        expect: Expect::Clean("add_liquidity"),
    },
    RealTx {
        label: "v2/addLiquidityETH (wallet-suffixed)",
        tx_hash: "0xd598608358c503c9ab2569fd1e03a441e6f9f2efcea7a205b99222bc27cc03de",
        bundle_json: B_V2_ADD_LIQ_ETH, chain_id: 8453,
        to: "0xcf77a3ba9a5ca399b7c97c74d54e5b1beb874e43",
        from: "0x5c98a04f663f32bb5e0778b1b9d8cb71cbdce3cb", value_wei: "15785375570670469818",
        calldata: "0xb7e0d4c0000000000000000000000000acfe6019ed1a7dc6f7b508c02d1b04ec88cc21bf000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006bfe2a5a072b12e8e9000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005c98a04f663f32bb5e0778b1b9d8cb71cbdce3cb000000000000000000000000000000000000000000000000000000006a0ec7748779ce964b87d3f89643854abe0162635f7239717a66386f700b0080218021802180218021802180218021",
        expect: Expect::Clean("add_liquidity"),
    },
    RealTx {
        label: "v2/removeLiquidity",
        tx_hash: "0x526b40df8202b83216c3a10fddeb0e41f86bc6f9def8bb51274a47f8918e037c",
        bundle_json: B_V2_REMOVE_LIQ, chain_id: 8453,
        to: "0xcf77a3ba9a5ca399b7c97c74d54e5b1beb874e43",
        from: "0x28aa4f9ffe21365473b64c161b566c3cdead0108", value_wei: "0",
        calldata: "0x0dede6c400000000000000000000000074ccbe53f77b08632ce0cb91d3a545bf6b8e0979000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda0291300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000204c7701f55227a000000000000000000000000000000000000000000011b3b21d87a5332b0f66000000000000000000000000000000000000000000000000000000003b5779a2000000000000000000000000028aa4f9ffe21365473b64c161b566c3cdead0108000000000000000000000000000000000000000000000000000000006a0ec4738779ce964b87d3f89643854abe0262635f7239717a66386f700b0080218021802180218021802180218021",
        expect: Expect::Clean("remove_liquidity"),
    },
    // ─── Slipstream SwapRouter — Phase A B-1 fix: `$.args.params[N]` → `$.args.<field>` ─
    RealTx {
        label: "slipstream/exactInputSingle",
        tx_hash: "0xe1b30d20b41873aba905daddedd34fe0b7cc05be15fc115549997e23a128250f",
        bundle_json: B_SLIP_EXACT_IN_SINGLE, chain_id: 8453,
        to: "0xbe6d8f0d05cc4be24d5167a3ef062215be6d18a5",
        from: "0xbbf09c739fdfc0408ba80fb6e6dcb72a1d4a1bfe", value_wei: "0",
        calldata: "0xa026383e000000000000000000000000370923d39f139c64813f173a1bf0b4f9ba36a24f000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda02913000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000bbf09c739fdfc0408ba80fb6e6dcb72a1d4a1bfe000000000000000000000000000000000000000000000000000000006a0ec53300000000000000000000000000000000000000000000338e9576d511b673856c0000000000000000000000000000000000000000000000000000000009ee69d00000000000000000000000000000000000000000000000000000000000000000",
        expect: Expect::Clean("swap"),
    },
    RealTx {
        label: "slipstream/exactInput",
        tx_hash: "0xa1e90df59ef4390fbe368eae7f80345f89a7e4cc484357fce96de1fcac05b01a",
        bundle_json: B_SLIP_EXACT_IN, chain_id: 8453,
        to: "0xbe6d8f0d05cc4be24d5167a3ef062215be6d18a5",
        from: "0xa73072adc6c34859426fcc29bc6ca2cac07c93c3", value_wei: "0",
        calldata: "0xc04b8d59000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000a73072adc6c34859426fcc29bc6ca2cac07c93c3000000000000000000000000000000000000000000000000000000006a0ec41d0000000000000000000000000000000000000000000000052164d29366f80000000000000000000000000000000000000000000000000000000000008468a58a00000000000000000000000000000000000000000000000000000000000000421bc0c42215582d5a085795f4badbac3ff36d1bcb0000c84200000000000000000000000000000000000006000064833589fcd6edb6e08f4c7c32d4f71b54bda02913000000000000000000000000000000000000000000000000000000000000",
        expect: Expect::Clean("swap"),
    },
    RealTx {
        label: "slipstream/exactOutputSingle",
        tx_hash: "0x4319ac2856017da7db57619d68026608d26611a34da9a1da25ff3977c8774e48",
        bundle_json: B_SLIP_EXACT_OUT_SINGLE, chain_id: 8453,
        to: "0xbe6d8f0d05cc4be24d5167a3ef062215be6d18a5",
        from: "0x3bf66b5ba807ec0e8faa33ac15c283e05dfad379", value_wei: "0",
        calldata: "0xc714e838000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda02913000000000000000000000000420000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000003bf66b5ba807ec0e8faa33ac15c283e05dfad379000000000000000000000000000000000000000000000000000000006a0ec3a9000000000000000000000000000000000000000000000000008e1bc9bf04000000000000000000000000000000000000000000000000000000000000052803880000000000000000000000000000000000000000000000000000000000000000",
        expect: Expect::Clean("swap"),
    },
    RealTx {
        label: "slipstream/exactOutput",
        tx_hash: "0xc5c94088156f98767b03f1a5ac1f112ff318c803e05d7c2793f72f3304024ec0",
        bundle_json: B_SLIP_EXACT_OUT, chain_id: 8453,
        to: "0xbe6d8f0d05cc4be24d5167a3ef062215be6d18a5",
        from: "0x8678f58ac6c4748b5289d0db70e627eef395dead", value_wei: "0",
        calldata: "0xf28c0498000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000008678f58ac6c4748b5289d0db70e627eef395dead000000000000000000000000000000000000000000000000000000006a0ec785000000000000000000000000000000000000000000002b91ebde529021ee7e260000000000000000000000000000000000000000000000000000000002fc157800000000000000000000000000000000000000000000000000000000000000424c87da04887a1f9f21f777e3a8dd55c3c9f847010000c84200000000000000000000000000000000000006000064833589fcd6edb6e08f4c7c32d4f71b54bda02913000000000000000000000000000000000000000000000000000000000000",
        expect: Expect::Clean("swap"),
    },
    // Phase E — Mock `LocalIndexChildResolver` wired into `Ctx::map_ctx`
    // mirrors production `declarative_route_request_json` + `WasmChildResolver`.
    // inner call (selector `0xa026383e`) → `slipstream/exactInputSingle@1.0.0` →
    // resolves to `swap` envelope. Phase A B-1 fix made the inner Slipstream
    // router bundles map cleanly; with the wire-up the multicall now classifies
    // Clean.
    RealTx {
        label: "slipstream/multicall",
        tx_hash: "0x7aff86a8e37c8c490ab6e6e04612c121f0ec9e7b80be62318e288a43a7e0ee57",
        bundle_json: B_SLIP_MULTICALL, chain_id: 8453,
        to: "0xbe6d8f0d05cc4be24d5167a3ef062215be6d18a5",
        from: "0x95f4665ccb2f1bf3d7c42f85ef4a88b9a68a1b59", value_wei: "61003943233485906",
        calldata: "0xac9650d80000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000104a026383e00000000000000000000000042000000000000000000000000000000000000060000000000000000000000004da9a0f397db1397902070f93a4d6ddbc0e0e6e800000000000000000000000000000000000000000000000000000000000000c800000000000000000000000095f4665ccb2f1bf3d7c42f85ef4a88b9a68a1b59000000000000000000000000000000000000000000000000000000006a0e8d2200000000000000000000000000000000000000000000000000d8bac35e547c5200000000000000000000000000000000000000000000000115473824344e0136000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
        expect: Expect::Clean("swap"),
    },
    // ─── Slipstream NPM — Phase A B-1 fix: `$.args.params[N]` → `$.args.<field>` ──
    RealTx {
        label: "slipstream-npm/mint",
        tx_hash: "0x8c47960548742870d21d34642559c4387b4419bca3821fbbab977b1d02d6049d",
        bundle_json: B_NPM_MINT, chain_id: 8453,
        to: "0x827922686190790b37229fd06084350e74485b72",
        from: "0x5ea80c1699bb786ade0e58d0b0c40ff2a6974bf0", value_wei: "0",
        calldata: "0xb5007d1f0000000000000000000000001111111111166b7fe7bd91427724b487980afc69000000000000000000000000833589fcd6edb6e08f4c7c32d4f71b54bda029130000000000000000000000000000000000000000000000000000000000000064fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb19b4fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffb1a1800000000000000000000000000000000000000000000046e71f8188435000000000000000000000000000000000000000000000000000000000000000ad01872000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005ea80c1699bb786ade0e58d0b0c40ff2a6974bf0000000000000000000000000000000000000000000000000000000006a0ec5010000000000000000000000000000000000000000000000000000000000000000",
        expect: Expect::Clean("mint_liquidity_nft"),
    },
    RealTx {
        label: "slipstream-npm/increaseLiquidity",
        tx_hash: "0x71655dbc0db032d1038b4874414300bf65594b9ac9bd2c090d7d2c45ffa047bf",
        bundle_json: B_NPM_INCREASE, chain_id: 8453,
        to: "0x827922686190790b37229fd06084350e74485b72",
        from: "0xbe2b5d6954e133a127ae37e871622b2bae533be1", value_wei: "0",
        calldata: "0x219f5d170000000000000000000000000000000000000000000000000000000001b21eda000000000000000000000000000000000000000000000ca21876e955737d827600000000000000000000000000000000000000000000000000000001dd58667500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006a0ec411",
        expect: Expect::Clean("increase_liquidity"),
    },
    RealTx {
        label: "slipstream-npm/decreaseLiquidity",
        tx_hash: "0xc301ac84af3122b3b715985bd34292ceabe95be187651781a7f6044405d4c1d9",
        bundle_json: B_NPM_DECREASE, chain_id: 8453,
        to: "0x827922686190790b37229fd06084350e74485b72",
        from: "0x74e28952d1d4910625ed1aae7137c0720f5bbb35", value_wei: "0",
        calldata: "0x0c49ccbe00000000000000000000000000000000000000000000000000000000043d182a000000000000000000000000000000000000000000000bdefcd883a5e82b4de400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006a0ed4ba",
        expect: Expect::Clean("decrease_liquidity"),
    },
    RealTx {
        label: "slipstream-npm/collect",
        tx_hash: "0xad0da3b288a2d0912e23f57019ff943be17efc0c80d104229ae7d55d39cc10a9",
        bundle_json: B_NPM_COLLECT, chain_id: 8453,
        to: "0x827922686190790b37229fd06084350e74485b72",
        from: "0xb11727bf13d29e20680f3fa74d15a8d76b33b430", value_wei: "0",
        calldata: "0xfc6f786500000000000000000000000000000000000000000000000000000000043c42d4000000000000000000000000b11727bf13d29e20680f3fa74d15a8d76b33b43000000000000000000000000000000000ffffffffffffffffffffffffffffffff00000000000000000000000000000000ffffffffffffffffffffffffffffffff",
        expect: Expect::Clean("claim_rewards"),
    },
    // Phase E — Mock resolver wired (see above). inner calls (selectors
    // `0x0c49ccbe` = decreaseLiquidity, `0xfc6f7865` = collect) resolve to the
    // matching slipstream-npm bundles; Phase A B-1 fix + Phase D collect-
    // unknown processing yield `[decrease_liquidity, claim_rewards]` envelopes.
    // `Expect::Clean` checks `envelopes[0].action.kind()` only — the second
    // envelope's kind is cross-checked via `field_checks_for`.
    RealTx {
        label: "slipstream-npm/multicall",
        tx_hash: "0x06b491c1afd407d58ebcd72d7a9e146637a78d5774bec569c6cccffd18b24df2",
        bundle_json: B_NPM_MULTICALL, chain_id: 8453,
        to: "0x827922686190790b37229fd06084350e74485b72",
        from: "0x9cabe00d0325ff1e8bae816ae18632c1c987582b", value_wei: "0",
        calldata: "0xac9650d8000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000012000000000000000000000000000000000000000000000000000000000000000a40c49ccbe0000000000000000000000000000000000000000000000000000000000c2d05e000000000000000000000000000000000000000000000000047d08db8887580b00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000006a0ec412000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000084fc6f78650000000000000000000000000000000000000000000000000000000000c2d05e0000000000000000000000009cabe00d0325ff1e8bae816ae18632c1c987582b000000000000000000000000000000000001bc16d674ec7ff21f494c589c0000000000000000000000000000000000000001bc16d674ec7ff21f494c589c000000000000000000000000000000000000000000000000000000000000",
        expect: Expect::Clean("decrease_liquidity"),
    },
    // ─── Universal Router ───────────────────────────────────────────────────
    RealTx {
        label: "universal-router/execute",
        tx_hash: "0x7c4fd4cf08fd12d363a14bfe91ec160c7b41ac44834d1bcdeb741b1e84119d68",
        bundle_json: B_UR_EXECUTE, chain_id: 8453,
        to: "0xcaf22ce31298cf2bf1d152862f80216478ad7c67",
        from: "0x07a0c4c00323f6a594ab2d501ae013a3dae4a33e", value_wei: "0",
        calldata: "0x3593564c000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000006a0ec4800000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000010000000000000000000000000007a0c4c00323f6a594ab2d501ae013a3dae4a33e000000000000000000000000000000000000000000001dcee09801f8680000000000000000000000000000000000000000000000000000000000000000317a1d00000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002b50d2280441372486beecdd328c1854743ebacb070800c8833589fcd6edb6e08f4c7c32d4f71b54bda02913000000000000000000000000000000000000000000",
        expect: Expect::Clean("swap"),
    },
    // T2-1 — UR no-deadline `execute(bytes,bytes[])` `0x24856bc3` overload, the
    // dominant Aerodrome UR traffic (Dune 267k/10d). Promoted from GAP_FIXTURES
    // by Phase A.2 — new `execute-no-deadline@1.0.0.json` bundle. Inner opcode
    // `0x01` = V3_SWAP_EXACT_OUT (payerIsSender=true).
    RealTx {
        label: "universal-router/execute(bytes,bytes[]) — no-deadline",
        tx_hash: "0x701d01476618cbb2c9007407812a634793f01d45ee561bef16b9a010abdd7c9b",
        bundle_json: B_UR_EXECUTE_NO_DEADLINE, chain_id: 8453,
        to: "0xc5b6786d7b64767d775877b0b6a319ad946b11b5",
        from: "0x4eff8063e497b5ef4214a614e5248a5e10c8f4f2", value_wei: "0",
        calldata: "0x24856bc300000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000101000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000001200000000000000000000000004eff8063e497b5ef4214a614e5248a5e10c8f4f20000000000000000000000000000000000000000000001117c7ac8f620bb62cc000000000000000000000000000000000000000000000000000000000d0c260600000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002b9126236476efba9ad8ab77855c60eb5bf37586eb080064833589fcd6edb6e08f4c7c32d4f71b54bda02913000000000000000000000000000000000000000000",
        expect: Expect::Clean("swap"),
    },
    // ─── Voter ──────────────────────────────────────────────────────────────
    RealTx {
        label: "voter/vote",
        tx_hash: "0xcd966a8be45a3461298c3d524cf3ba48aa2b99053c709eb48c33b437b478b914",
        bundle_json: B_VOTER_VOTE, chain_id: 8453,
        to: "0x16613524e02ad97edfef371bc883f2f5d6c480a5",
        from: "0xe174a2daaf200e0495aeb41fc1e59f9614654fc2", value_wei: "0",
        calldata: "0x7ac09bf7000000000000000000000000000000000000000000000000000000000001d062000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000000000001000000000000000000000000ef7e596aef9e4c6301b4d1f1e88f8ffe8c30622200000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000056bc75e2d631000008779ce964b87d3f89643854abe0162635f7239717a66386f700b0080218021802180218021802180218021",
        expect: Expect::Clean("gauge_vote"),
    },
    RealTx {
        label: "voter/claimBribes",
        tx_hash: "0x05c7b962f67075d1c2fd062ea080e55a79c9bb678eeeaaefbae93793cb83c885",
        bundle_json: B_VOTER_CLAIM_BRIBES, chain_id: 8453,
        to: "0x16613524e02ad97edfef371bc883f2f5d6c480a5",
        from: "0x9a589729132a053e6bed0fbfe97a75cc7094fd07", value_wei: "0",
        calldata: "0x7715ee75000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000141690000000000000000000000000000000000000000000000000000000000000001000000000000000000000000cec4c9f15b0530a50fba05862ea403c26825ef4d0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000004c87da04887a1f9f21f777e3a8dd55c3c9f847018779ce964b87d3f89643854abe0262635f7239717a66386f700b0080218021802180218021802180218021",
        // B-2 (Phase B fix): claim_rewards lowering inserts `sourceLabel`;
        // the ClaimRewardsContext cedarschema now declares it as
        // `sourceLabel?: String` (optional) so evaluate accepts the context.
        // Was previously `Expect::EvaluateFault("sourceLabel")`.
        expect: Expect::Clean("claim_rewards"),
    },
    // ─── VotingEscrow ───────────────────────────────────────────────────────
    RealTx {
        label: "voting-escrow/createLock",
        tx_hash: "0xd38e14d9171bd0e7aaa2ea4cfc713cbb8f9d8e740b888e2787c989078bc4eba5",
        bundle_json: B_VE_CREATE_LOCK, chain_id: 8453,
        to: "0xebf418fe2512e7e6bd9b87a8f0f294acdc67e6b4",
        from: "0x0fe8a3ff06996db01ed5add020453de99548edce", value_wei: "0",
        calldata: "0xb52c05fe000000000000000000000000000000000000000000000000003328b944c400000000000000000000000000000000000000000000000000000000000002c7ea00",
        expect: Expect::Clean("lock_create"),
    },
    RealTx {
        label: "voting-escrow/increaseAmount",
        tx_hash: "0x44ee5a1485e50befb09b3f9f02e125b1afef52a70990450f6d0ee5951c32b115",
        bundle_json: B_VE_INCREASE_AMOUNT, chain_id: 8453,
        to: "0xebf418fe2512e7e6bd9b87a8f0f294acdc67e6b4",
        from: "0x540d1cc67d61c54b6830c853d5fb79f43543eaaa", value_wei: "0",
        calldata: "0xb2383e55000000000000000000000000000000000000000000000000000000000001cc730000000000000000000000000000000000000000000000056915c288825a3c908779ce964b87d3f89643854abe0262635f7239717a66386f700b0080218021802180218021802180218021",
        expect: Expect::Clean("lock_increase"),
    },
    RealTx {
        label: "voting-escrow/merge",
        tx_hash: "0x584f9aaaa43b673d34517aaae607c5a2a09073f245157394ed41ba856215a803",
        bundle_json: B_VE_MERGE, chain_id: 8453,
        to: "0xebf418fe2512e7e6bd9b87a8f0f294acdc67e6b4",
        from: "0x4ef74aef9e01a035f4f74fb1f6d8e39b3027ca2e", value_wei: "0",
        calldata: "0xd1c2babb000000000000000000000000000000000000000000000000000000000001d489000000000000000000000000000000000000000000000000000000000001d48a8779ce964b87d3f89643854abe0262635f7239717a66386f700b0080218021802180218021802180218021",
        expect: Expect::Clean("lock_manage"),
    },
    // ─── RewardsDistributor ─────────────────────────────────────────────────
    RealTx {
        label: "rewards-distributor/claim",
        tx_hash: "0x65b292ed3d5115666f5ed624d4b76b042298ae23d22d198055d4fb8e1b207b6b",
        bundle_json: B_REWARDS_CLAIM, chain_id: 8453,
        to: "0x227f65131a261548b057215bb1d5ab2997964c7d",
        from: "0x4227185aac699ffb5d18707ebc46ee5568370151", value_wei: "0",
        calldata: "0x379607f5000000000000000000000000000000000000000000000000000000000000988b8779ce964b87d3f89643854abe0262635f7239717a66386f700b0080218021802180218021802180218021",
        // B-2 (Phase B fix): same sourceLabel schema drift, now resolved by
        // the `sourceLabel?: String` optional addition.
        expect: Expect::Clean("claim_rewards"),
    },
    // ─── Gauge ──────────────────────────────────────────────────────────────
    RealTx {
        label: "gauge/getReward (wallet-suffixed)",
        tx_hash: "0xaf7e1a52294b19ce47b2a5f2aae38371a211d21cb92d4adf852a6eaa6b40caaa",
        bundle_json: B_GAUGE_GET_REWARD, chain_id: 8453,
        to: "0x4f09bab2f0e15e2a078a227fe1537665f55b8360",
        from: "0xaecf89718604b2edb5b7fbe6203448755b7d9525", value_wei: "0",
        calldata: "0xc00007b0000000000000000000000000aecf89718604b2edb5b7fbe6203448755b7d95258779ce964b87d3f89643854abe0162635f7239717a66386f700b0080218021802180218021802180218021",
        // B-2 (Phase B fix): same sourceLabel schema drift, now resolved.
        expect: Expect::Clean("claim_rewards"),
    },
    RealTx {
        label: "gauge/deposit (wallet-suffixed)",
        tx_hash: "0x32ae7783d5ec07d57aa246ced9052bc0e8939e2c7aa6a5a37a3d6b329434d520",
        bundle_json: B_GAUGE_DEPOSIT, chain_id: 8453,
        to: "0x519bbd1dd8c6a94c46080e24f316c14ee758c025",
        from: "0xa8b9b8b02f1caf1b7a9825eb7c568e58eea8eca0", value_wei: "0",
        calldata: "0xb6b55f2500000000000000000000000000000000000000000000000000000034c992ecfd8779ce964b87d3f89643854abe0262635f7239717a66386f700b0080218021802180218021802180218021",
        expect: Expect::Clean("lp_stake"),
    },
];

// ───────────────────────────────────────────────────────────────────────────
// No-callkey gap fixtures — verified-uncovered real selectors. The ratchet:
// these MUST have no by-callkey index entry. If someone adds coverage the
// test breaks, prompting promotion to a full `RealTx` fixture.
// ───────────────────────────────────────────────────────────────────────────

struct GapTx {
    label: &'static str,
    tx_hash: &'static str,
    chain_id: u64,
    to: &'static str,
    /// `"0x" + 8 hex` selector.
    selector: &'static str,
    /// Decoded signature (provenance — what real users are signing uncovered).
    signature: &'static str,
}

const GAP_FIXTURES: &[GapTx] = &[
    // T2-1 `universal-router/execute(bytes,bytes[]) — no-deadline` promoted to a
    // RealTx fixture by Phase A.2 (new execute-no-deadline@1.0.0.json bundle).
    //
    // T2-2~T2-5 (ve(3,3) managed/permanent) — Phase D added the 4 new bundles
    // (voter/depositManaged · voter/withdrawManaged · voting-escrow/lockPermanent
    // · voting-escrow/unlockPermanent), so by-callkey now resolves. RealTx
    // promotion deferred until calldata is captured from Basescan; the bundles'
    // emit semantics (lock_manage with merge/split kind reused — schema enum has
    // only Merge/Split variants, so depositManaged uses merge and withdraw/
    // permanent paths use split as placeholder kinds) are exercised by future
    // V2.C real-tx fixtures.
    GapTx {
        label: "slipstream/sweepToken",
        tx_hash: "0x21348aacc0ab7eac047ef4c8042e89590679e9e4f25fc17d51f91889c44feb75",
        chain_id: 8453,
        to: "0xbe6d8f0d05cc4be24d5167a3ef062215be6d18a5",
        selector: "0xdf2ab5bb",
        signature: "sweepToken(address,uint256,address)",
    },
];

// ───────────────────────────────────────────────────────────────────────────
// Tests
// ───────────────────────────────────────────────────────────────────────────

/// Run every real-tx fixture through the 4-stage pipeline, print a one-line
/// classification (the coverage report), and assert each matches its
/// documented `expect`. Collects ALL mismatches so one run shows the full
/// picture.
#[test]
fn real_tx_classification() {
    let mut mismatches = Vec::new();
    let mut field_checks = 0usize;
    for tx in FIXTURES {
        let (routed, report) = run_pipeline(tx);
        println!(
            "[real-tx] {:<36} routed={:<5} tx={}  {}",
            tx.label,
            routed,
            tx.tx_hash,
            report.tag()
        );
        if !routed {
            mismatches.push(format!(
                "{}: by-callkey index has no entry (covered fixture must route)",
                tx.label
            ));
        }
        if !report_matches(&report, &tx.expect) {
            mismatches.push(format!(
                "{}: expected {:?}, got {}",
                tx.label,
                tx.expect,
                report.tag()
            ));
        }
        // Stage 3 deep check — for Clean fixtures, cross-check the envelope's
        // permission-surface fields against the independent `cast` ground truth.
        if let Report::Clean { envelope_json, .. } = &report {
            let checks = field_checks_for(tx.label);
            if checks.is_empty() {
                mismatches.push(format!("{}: Clean 인데 field_checks 미정의", tx.label));
            }
            let env: serde_json::Value =
                serde_json::from_str(envelope_json).expect("envelope json parses");
            for (ptr, expected) in checks {
                field_checks += 1;
                let got = env.pointer(ptr).and_then(serde_json::Value::as_str);
                if got.map(|g| g.eq_ignore_ascii_case(expected)) != Some(true) {
                    mismatches.push(format!(
                        "{} field {ptr}: expected `{expected}`, got {got:?}",
                        tx.label
                    ));
                }
            }
        }
    }
    println!("[real-tx] Stage-3 field cross-checks run: {field_checks}");
    assert!(
        mismatches.is_empty(),
        "real-tx classification mismatches ({}):\n  {}",
        mismatches.len(),
        mismatches.join("\n  ")
    );
}

/// **Stage 4 policy-gating** — a real V2 swap (calldata `to` = `0x0`, ≠ signer)
/// evaluated against the swap recipient self-guard must produce a deny-severity
/// `Fail` with the guard matched. Proves the verdict engine gates on a real
/// decoded field, not merely that `evaluate` did not fault.
#[test]
fn policy_gating_swap_recipient_guard() {
    let tx = FIXTURES
        .iter()
        .find(|t| t.label == "v2/swapExactTokensForTokens")
        .expect("v2 swap fixture present");
    match evaluate_with_policy(tx, POLICY_SWAP_RECIPIENT_GUARD) {
        Verdict::Fail(matched) => assert!(
            matched
                .iter()
                .any(|p| p.policy_id.contains("swap-recipient-self")),
            "expected swap recipient guard to match, got {matched:?}"
        ),
        other => panic!("expected Verdict::Fail (recipient 0x0 != signer), got {other:?}"),
    }
}

/// Ratchet: every verified-uncovered selector must have **no** by-callkey
/// index entry. If coverage is added this breaks — promote to a `RealTx`.
#[test]
fn gap_no_callkey_ratchet() {
    let mut regressions = Vec::new();
    for gap in GAP_FIXTURES {
        let callkey = format!(
            "{}__{}__{}.json",
            gap.chain_id,
            gap.to.to_lowercase(),
            gap.selector
        );
        let path = registry_root().join("index/by-callkey").join(&callkey);
        let exists = path.exists();
        println!(
            "[gap] {:<46} {:<38} tx={} callkey-exists={}",
            gap.label, gap.signature, gap.tx_hash, exists
        );
        if exists {
            regressions.push(format!(
                "{}: callkey {} now exists — promote to a RealTx fixture",
                gap.label, callkey
            ));
        }
    }
    assert!(
        regressions.is_empty(),
        "gap ratchet — coverage added, fixtures need promotion:\n  {}",
        regressions.join("\n  ")
    );
}
