//! Aerodrome V2 Router edge case integration tests (Phase 8 — A-TEST-AERO-V2).
//!
//! Round 4 introduced 8 Aerodrome V2 Router bundles under
//! `registry/manifests/aerodrome/router-v2/`. The router replaces Uniswap V2's
//! `address[] path` with a `tuple[] routes` whose tuple fields are
//! `(from, to, stable, factory)`. These tests pin the declarative path's
//! handling of that new shape, with focus on:
//!
//!   * **Endpoint extraction** — `$.args.routes[0][0]` (first hop's `from`)
//!     and `$.args.routes[-1][1]` (last hop's `to`) must resolve correctly
//!     across single, multi, and stress-length route arrays.
//!   * **Stable flag** — the bundles intentionally **do not** emit the
//!     `stable` boolean today. The tests assert this absence so any future
//!     dialect addition (`adapter:aerodrome.stable_pool` etc.) is a
//!     deliberate change, not an accidental drift.
//!   * **Factory address** — same as stable: the routes tuple carries a
//!     per-hop `factory` field, but the V1.0.0 bundles drop it. Tests
//!     document this surface explicitly.
//!   * **Native legs** — `swapExactETHForTokens` (input from `$.tx.value_wei`)
//!     and `removeLiquidityETHSupportingFeeOnTransferTokens` (`outputTokens[1].kind=native`).
//!   * **Boundary amounts** — zero amountIn passes through verbatim (Cedar
//!     enforces non-zero policies, not the declarative interpreter).
//!
//! Production code is unchanged — this file only adds test coverage.
//! Bundle JSON is loaded via `include_str!` from `registry/manifests/`.

use std::str::FromStr as _;

use abi_resolver::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
use alloy_primitives::U256;
use mappers::declarative::{types::AdapterFunctionBundle, DeclarativeMapper};
use mappers::mapper::{MapContext, Mapper};
use mappers::EmptyTokenRegistry;
use policy_engine::action::dex::RemoveLiquidityExitMode;
use policy_engine::action::dex::SwapMode;
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountKind, AssetKind, DecimalString,
};
use policy_engine::{policy_request_from_envelope, PolicyEngineBuilder, Verdict};

// ───────────────────────────────────────────────────────────────────────────
// Aerodrome V2 Router bundle fixtures (registry/manifests/aerodrome/router-v2/*).
// Loaded directly so the tests track whatever the registry ships, no
// drift between fixture and production.
// ───────────────────────────────────────────────────────────────────────────

const AERO_SWAP_EXACT_TOKENS_FOR_TOKENS: &str = include_str!(
    "../../../../registry/manifests/aerodrome/router-v2/swapExactTokensForTokens@1.0.0.json"
);
const AERO_SWAP_EXACT_ETH_FOR_TOKENS: &str = include_str!(
    "../../../../registry/manifests/aerodrome/router-v2/swapExactETHForTokens@1.0.0.json"
);
const AERO_ADD_LIQUIDITY: &str =
    include_str!("../../../../registry/manifests/aerodrome/router-v2/addLiquidity@1.0.0.json");
const AERO_REMOVE_LIQUIDITY_ETH_FOT: &str = include_str!(
    "../../../../registry/manifests/aerodrome/router-v2/removeLiquidityETHSupportingFeeOnTransferTokens@1.0.0.json"
);

// ───────────────────────────────────────────────────────────────────────────
// Address fixtures — canonical Base mainnet checksum, lowercased per
// `policy_engine::action::Address::from_str` normalisation.
// ───────────────────────────────────────────────────────────────────────────

fn usdc() -> Address {
    // USDC on Base.
    Address::from_str("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()
}

fn usdt() -> Address {
    // USDT (placeholder address — only structure matters for these tests).
    Address::from_str("0xdac17f958d2ee523a2206206994597c13d831ec7").unwrap()
}

fn weth() -> Address {
    // WETH on Base.
    Address::from_str("0x4200000000000000000000000000000000000006").unwrap()
}

fn usdbc() -> Address {
    // USDbC (bridged USDC on Base).
    Address::from_str("0xd9aaec86b65d86f6a7b5b1b0c42ffa531710b6ca").unwrap()
}

fn recipient() -> Address {
    Address::from_str("0x4444444444444444444444444444444444444444").unwrap()
}

/// The tx signer used by `Ctx` — the `principal` (signing wallet) of every
/// lowered policy request. P1 (`add-liquidity-recipient-self`, T12/T13)
/// compares `context.recipient` against this through `principal.address`.
fn signer() -> Address {
    Address::from_str("0x00000000000000000000000000000000000000aa").unwrap()
}

/// Default Aerodrome `PoolFactory` on Base — used as the canonical `factory`
/// inside each route tuple. Source: Aerodrome docs (`Aerodrome Pool Factory`).
fn aerodrome_default_factory() -> Address {
    Address::from_str("0x420dd381b31aef6683db6b902084cb0ffece40da").unwrap()
}

/// Custom factory placeholder for the "factory != default" test scenario.
fn custom_factory() -> Address {
    Address::from_str("0x9999999999999999999999999999999999999999").unwrap()
}

// ───────────────────────────────────────────────────────────────────────────
// MapContext helper — Base chain (8453), empty token registry.
// ───────────────────────────────────────────────────────────────────────────

struct Ctx {
    registry: EmptyTokenRegistry,
    from: Address,
    to: Address,
    value: DecimalString,
}

impl Ctx {
    fn new() -> Self {
        Self::with_value("0")
    }

    fn with_value(value_wei: &str) -> Self {
        Self {
            registry: EmptyTokenRegistry,
            from: signer(),
            to: Address::from_str("0xcf77a3ba9a5ca399b7c97c74d54e5b1beb874e43").unwrap(),
            value: DecimalString::from_str(value_wei).unwrap(),
        }
    }

    fn map_ctx(&self) -> MapContext<'_> {
        MapContext::new(
            8453, // Base mainnet — matches bundle.match.chain_ids[0]
            &self.from,
            &self.to,
            &self.value,
            Some(1_700_000_000),
            &self.registry,
        )
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Decoded call builders mirroring `bridge.rs::flatten_tuple_arg`.
//
// The Aerodrome `routes` arg has type `tuple[]` where each tuple is
// `(address from, address to, bool stable, address factory)`. The bridge
// flattens the top-level args but leaves nested tuples as
// `DecodedValue::Tuple([...])`. `decoded_value_to_json` then maps each tuple
// to a JSON array, so `$.args.routes[0][0]` indexes into the first tuple's
// first element (`from`), and `$.args.routes[-1][1]` walks to the last
// tuple's `to`.
// ───────────────────────────────────────────────────────────────────────────

/// Build one route tuple `(from, to, stable, factory)` as `DecodedValue::Tuple`.
fn route(from: Address, to: Address, stable: bool, factory: Address) -> DecodedValue {
    DecodedValue::Tuple(vec![
        DecodedValue::Address(from),
        DecodedValue::Address(to),
        DecodedValue::Bool(stable),
        DecodedValue::Address(factory),
    ])
}

/// Build a `DecodedCall` for Aerodrome `swapExactTokensForTokens` with the
/// given route list and amounts. Mirrors the post-bridge flatten layout.
fn aerodrome_swap_decoded(
    decoder_id: DecoderId,
    amount_in: U256,
    amount_out_min: U256,
    routes: Vec<DecodedValue>,
    recipient_addr: Address,
) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature:
            "swapExactTokensForTokens(uint256,uint256,(address,address,bool,address)[],address,uint256)".into(),
        args: vec![
            DecodedArg {
                name: "amountIn".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(amount_in),
            },
            DecodedArg {
                name: "amountOutMin".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(amount_out_min),
            },
            DecodedArg {
                name: "routes".into(),
                abi_type: "(address,address,bool,address)[]".into(),
                value: DecodedValue::Array(routes),
            },
            DecodedArg {
                name: "to".into(),
                abi_type: "address".into(),
                value: DecodedValue::Address(recipient_addr),
            },
            DecodedArg {
                name: "deadline".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(U256::from(1_700_000_900_u64)),
            },
        ],
        nested: vec![],
    }
}

/// Build a `DecodedCall` for Aerodrome `swapExactETHForTokens` (no `amountIn`
/// arg — native input is sourced from `$.tx.value_wei`).
fn aerodrome_swap_eth_decoded(
    decoder_id: DecoderId,
    amount_out_min: U256,
    routes: Vec<DecodedValue>,
    recipient_addr: Address,
) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature:
            "swapExactETHForTokens(uint256,(address,address,bool,address)[],address,uint256)".into(),
        args: vec![
            DecodedArg {
                name: "amountOutMin".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(amount_out_min),
            },
            DecodedArg {
                name: "routes".into(),
                abi_type: "(address,address,bool,address)[]".into(),
                value: DecodedValue::Array(routes),
            },
            DecodedArg {
                name: "to".into(),
                abi_type: "address".into(),
                value: DecodedValue::Address(recipient_addr),
            },
            DecodedArg {
                name: "deadline".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(U256::from(1_700_000_900_u64)),
            },
        ],
        nested: vec![],
    }
}

/// Build a `DecodedCall` for Aerodrome `addLiquidity`. `stable` is a flat
/// bool arg here, not nested in a tuple — Aerodrome only nests `stable`
/// inside `routes` for swap functions.
fn aerodrome_add_liquidity_decoded(
    decoder_id: DecoderId,
    token_a: Address,
    token_b: Address,
    stable: bool,
    amount_a_desired: U256,
    amount_b_desired: U256,
    recipient_addr: Address,
) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature:
            "addLiquidity(address,address,bool,uint256,uint256,uint256,uint256,address,uint256)"
                .into(),
        args: vec![
            DecodedArg {
                name: "tokenA".into(),
                abi_type: "address".into(),
                value: DecodedValue::Address(token_a),
            },
            DecodedArg {
                name: "tokenB".into(),
                abi_type: "address".into(),
                value: DecodedValue::Address(token_b),
            },
            DecodedArg {
                name: "stable".into(),
                abi_type: "bool".into(),
                value: DecodedValue::Bool(stable),
            },
            DecodedArg {
                name: "amountADesired".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(amount_a_desired),
            },
            DecodedArg {
                name: "amountBDesired".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(amount_b_desired),
            },
            DecodedArg {
                name: "amountAMin".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(U256::from(1_u64)),
            },
            DecodedArg {
                name: "amountBMin".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(U256::from(1_u64)),
            },
            DecodedArg {
                name: "to".into(),
                abi_type: "address".into(),
                value: DecodedValue::Address(recipient_addr),
            },
            DecodedArg {
                name: "deadline".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(U256::from(1_700_000_900_u64)),
            },
        ],
        nested: vec![],
    }
}

/// Build a `DecodedCall` for `removeLiquidityETHSupportingFeeOnTransferTokens`.
fn aerodrome_remove_liquidity_eth_fot_decoded(
    decoder_id: DecoderId,
    token: Address,
    stable: bool,
    liquidity: U256,
    amount_token_min: U256,
    amount_eth_min: U256,
    recipient_addr: Address,
) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature:
            "removeLiquidityETHSupportingFeeOnTransferTokens(address,bool,uint256,uint256,uint256,address,uint256)".into(),
        args: vec![
            DecodedArg {
                name: "token".into(),
                abi_type: "address".into(),
                value: DecodedValue::Address(token),
            },
            DecodedArg {
                name: "stable".into(),
                abi_type: "bool".into(),
                value: DecodedValue::Bool(stable),
            },
            DecodedArg {
                name: "liquidity".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(liquidity),
            },
            DecodedArg {
                name: "amountTokenMin".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(amount_token_min),
            },
            DecodedArg {
                name: "amountETHMin".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(amount_eth_min),
            },
            DecodedArg {
                name: "to".into(),
                abi_type: "address".into(),
                value: DecodedValue::Address(recipient_addr),
            },
            DecodedArg {
                name: "deadline".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(U256::from(1_700_000_900_u64)),
            },
        ],
        nested: vec![],
    }
}

fn load_mapper(bundle_json: &str) -> DeclarativeMapper {
    let bundle: AdapterFunctionBundle =
        serde_json::from_str(bundle_json).expect("Aerodrome bundle parses");
    DeclarativeMapper::new(bundle)
}

fn unwrap_swap(envelope: &ActionEnvelope) -> &policy_engine::action::dex::SwapAction {
    match &envelope.action {
        Action::Swap(s) => s,
        other => panic!("expected SwapAction, got {other:?}"),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Tests
// ───────────────────────────────────────────────────────────────────────────

/// **T1: single-hop Route (length 1)**.
///
/// `routes = [(USDC, USDT, false, factory)]`. The bundle binds
/// `inputToken.asset.address = $.args.routes[0][0]` (first tuple's `from`)
/// and `outputToken.asset.address = $.args.routes[-1][1]` (last tuple's
/// `to`). For a single-hop route both bindings touch the same tuple but
/// different fields.
#[test]
fn aerodrome_swap_single_hop_route_resolves_endpoints() {
    let mapper = load_mapper(AERO_SWAP_EXACT_TOKENS_FOR_TOKENS);
    let ctx = Ctx::new();
    let routes = vec![route(usdc(), usdt(), false, aerodrome_default_factory())];

    let decoded = aerodrome_swap_decoded(
        mapper.declarative_decoder_id(),
        U256::from(1_000_000_u64),
        U256::from(900_000_u64),
        routes,
        recipient(),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("single-hop swap maps");
    assert_eq!(envelopes.len(), 1);
    let action = unwrap_swap(&envelopes[0]);

    assert_eq!(action.swap_mode, SwapMode::ExactIn);
    assert_eq!(action.input_token.asset.kind, AssetKind::Erc20);
    assert_eq!(action.input_token.asset.address, Some(usdc()));
    assert_eq!(action.output_token.asset.kind, AssetKind::Erc20);
    assert_eq!(action.output_token.asset.address, Some(usdt()));
    assert_eq!(action.recipient, recipient());
    assert_eq!(
        action.fee_bps, None,
        "Aerodrome bundle does not bind fee_bps"
    );
}

/// **T2: multi-hop Route (length 2)**.
///
/// `routes = [(USDC, WETH, false, f), (WETH, USDT, false, f)]`. `routes[0][0]`
/// (USDC) and `routes[-1][1]` (USDT) must straddle the entire path; the
/// intermediate WETH never surfaces on the envelope.
#[test]
fn aerodrome_swap_multi_hop_route_uses_first_and_last_endpoints() {
    let mapper = load_mapper(AERO_SWAP_EXACT_TOKENS_FOR_TOKENS);
    let ctx = Ctx::new();
    let factory = aerodrome_default_factory();
    let routes = vec![
        route(usdc(), weth(), false, factory.clone()),
        route(weth(), usdt(), false, factory),
    ];

    let decoded = aerodrome_swap_decoded(
        mapper.declarative_decoder_id(),
        U256::from(5_000_000_u64),
        U256::from(4_900_000_u64),
        routes,
        recipient(),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("multi-hop swap maps");
    let action = unwrap_swap(&envelopes[0]);

    assert_eq!(
        action.input_token.asset.address,
        Some(usdc()),
        "$.args.routes[0][0] must be the first hop's `from`"
    );
    assert_eq!(
        action.output_token.asset.address,
        Some(usdt()),
        "$.args.routes[-1][1] must be the last hop's `to`"
    );
    // WETH must not leak — it only appears as an intermediate.
    assert_ne!(action.input_token.asset.address, Some(weth()));
    assert_ne!(action.output_token.asset.address, Some(weth()));
}

/// **T3: 5-hop Route (max-chain stress)**.
///
/// Verifies `$.args.routes[-1][1]` correctly walks to the last tuple even
/// for a long route. Aerodrome enforces no hard route-length cap on the
/// router contract, but 5 hops is well beyond realistic frontend usage.
#[test]
fn aerodrome_swap_five_hop_route_uses_last_to() {
    let mapper = load_mapper(AERO_SWAP_EXACT_TOKENS_FOR_TOKENS);
    let ctx = Ctx::new();
    let factory = aerodrome_default_factory();

    // 5 hops: A -> B -> C -> D -> E -> F. Endpoints are the first `from`
    // (A) and last `to` (F).
    let hop_a = Address::from_str("0x1111111111111111111111111111111111111111").unwrap();
    let hop_b = Address::from_str("0x2222222222222222222222222222222222222222").unwrap();
    let hop_c = Address::from_str("0x3333333333333333333333333333333333333333").unwrap();
    let hop_d = Address::from_str("0x4444444444444444444444444444444444444444").unwrap();
    let hop_e = Address::from_str("0x5555555555555555555555555555555555555555").unwrap();
    let hop_f = Address::from_str("0x6666666666666666666666666666666666666666").unwrap();

    let routes = vec![
        route(hop_a.clone(), hop_b.clone(), false, factory.clone()),
        route(hop_b.clone(), hop_c.clone(), true, factory.clone()),
        route(hop_c.clone(), hop_d.clone(), false, factory.clone()),
        route(hop_d.clone(), hop_e.clone(), true, factory.clone()),
        route(hop_e.clone(), hop_f.clone(), false, factory),
    ];

    let decoded = aerodrome_swap_decoded(
        mapper.declarative_decoder_id(),
        U256::from(1_000_u64),
        U256::from(1_u64),
        routes,
        recipient(),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("5-hop swap maps");
    let action = unwrap_swap(&envelopes[0]);
    assert_eq!(action.input_token.asset.address, Some(hop_a));
    assert_eq!(action.output_token.asset.address, Some(hop_f));
}

/// **T4: `stable = true` on a route is dropped by the V1.0.0 bundle**.
///
/// The Aerodrome `routes` tuple's third field is the `stable` boolean. The
/// current V1.0.0 swap bundle does **not** bind it to any envelope field —
/// the envelope only carries endpoint addresses + amounts + recipient +
/// validity. This test pins the behaviour so a future dialect addition
/// (e.g. `adapter:aerodrome.stable_pool`) is a deliberate, reviewable change.
#[test]
fn aerodrome_swap_stable_true_is_not_in_envelope() {
    let mapper = load_mapper(AERO_SWAP_EXACT_TOKENS_FOR_TOKENS);
    let ctx = Ctx::new();
    // Stablecoin pair using the Aerodrome `stable=true` pool.
    let routes = vec![route(usdc(), usdbc(), true, aerodrome_default_factory())];

    let decoded = aerodrome_swap_decoded(
        mapper.declarative_decoder_id(),
        U256::from(1_000_000_u64),
        U256::from(998_000_u64),
        routes,
        recipient(),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("stable-pool swap maps");
    let envelope = &envelopes[0];
    let action = unwrap_swap(envelope);

    // Envelope endpoints reflect the route — stable flag does NOT affect them.
    assert_eq!(action.input_token.asset.address, Some(usdc()));
    assert_eq!(action.output_token.asset.address, Some(usdbc()));

    // Confirm the JSON serialisation has no `stable` / `stablePool` /
    // `adapter:aerodrome.*` key anywhere. Round-tripping via serde catches
    // any future field surfacing as a Cedar-visible attribute.
    let json = serde_json::to_value(envelope).expect("envelope serialises");
    let json_str = json.to_string();
    assert!(
        !json_str.contains("stable"),
        "V1.0.0 envelope must not emit `stable` flag, got JSON: {json_str}"
    );
    assert!(
        !json_str.contains("adapter:aerodrome"),
        "V1.0.0 envelope must not emit `adapter:aerodrome.*` dialect, got: {json_str}"
    );
}

/// **T5: `stable = false` on a route — same drop behaviour as T4**.
///
/// Companion to T4 covering the volatile (`stable=false`) pool branch.
/// Asserts both branches of the bool are treated identically by the
/// V1.0.0 bundle (i.e. ignored). Together with T4 they prevent silent
/// "only one branch wired" regressions.
#[test]
fn aerodrome_swap_stable_false_is_not_in_envelope() {
    let mapper = load_mapper(AERO_SWAP_EXACT_TOKENS_FOR_TOKENS);
    let ctx = Ctx::new();
    let routes = vec![route(usdc(), weth(), false, aerodrome_default_factory())];

    let decoded = aerodrome_swap_decoded(
        mapper.declarative_decoder_id(),
        U256::from(1_000_000_u64),
        U256::from(900_000_u64),
        routes,
        recipient(),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("volatile-pool swap maps");
    let envelope = &envelopes[0];
    let action = unwrap_swap(envelope);

    assert_eq!(action.input_token.asset.address, Some(usdc()));
    assert_eq!(action.output_token.asset.address, Some(weth()));

    let json = serde_json::to_value(envelope).expect("envelope serialises");
    let json_str = json.to_string();
    assert!(
        !json_str.contains("stable"),
        "stable=false envelope must also omit any `stable` key, got: {json_str}"
    );
}

/// **T6: custom `factory != default` is dropped by the V1.0.0 bundle**.
///
/// The Aerodrome routes tuple's fourth field allows per-hop factory override
/// (Velodrome-style multi-factory routing). The V1.0.0 bundle has no field
/// binding for `factory` — endpoint addresses come from `[0]`/`[1]`
/// positions only. This test pins that a custom factory passes through
/// without leaking into the envelope, so a Cedar policy that wants to gate
/// on factory must wait for a future dialect addition.
#[test]
fn aerodrome_swap_custom_factory_not_emitted() {
    let mapper = load_mapper(AERO_SWAP_EXACT_TOKENS_FOR_TOKENS);
    let ctx = Ctx::new();
    // Use a non-default factory address — should still map cleanly, but the
    // factory must not surface anywhere in the serialised envelope.
    let routes = vec![route(usdc(), usdt(), false, custom_factory())];

    let decoded = aerodrome_swap_decoded(
        mapper.declarative_decoder_id(),
        U256::from(1_000_000_u64),
        U256::from(900_000_u64),
        routes,
        recipient(),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("custom-factory swap maps");
    let envelope = &envelopes[0];

    let json = serde_json::to_value(envelope).expect("envelope serialises");
    let json_str = json.to_string();
    // The custom factory address must not appear as a value anywhere.
    assert!(
        !json_str.contains(&custom_factory().to_string()),
        "custom factory address must not be emitted in V1.0.0 envelope, got: {json_str}"
    );
    assert!(
        !json_str.contains("factory"),
        "envelope must not carry any `factory` key, got: {json_str}"
    );
}

/// **T7: `addLiquidity` with `stable = true` — flag dropped, tokens preserved**.
///
/// Same surface as T4/T5 but on the `addLiquidity` bundle (different
/// strategy: `add_liquidity` action). The `stable` arg is a flat bool here,
/// not nested in a routes tuple. The bundle is expected to emit both input
/// tokens via `$.args.tokenA` / `$.args.tokenB` and ignore the `stable` flag.
#[test]
fn aerodrome_add_liquidity_stable_true_preserves_inputs_drops_flag() {
    let mapper = load_mapper(AERO_ADD_LIQUIDITY);
    let ctx = Ctx::new();

    let decoded = aerodrome_add_liquidity_decoded(
        mapper.declarative_decoder_id(),
        usdc(),
        usdbc(),
        true, // stable pool
        U256::from(1_000_000_u64),
        U256::from(1_000_000_u64),
        recipient(),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("addLiquidity maps");
    let envelope = &envelopes[0];
    let Action::AddLiquidity(action) = &envelope.action else {
        panic!("expected Action::AddLiquidity, got {:?}", envelope.action);
    };

    assert_eq!(action.inputs.len(), 2);
    assert_eq!(action.inputs[0].asset.kind, AssetKind::Erc20);
    assert_eq!(action.inputs[0].asset.address, Some(usdc()));
    assert_eq!(action.inputs[0].amount.kind, AmountKind::Max);
    assert_eq!(action.inputs[1].asset.kind, AssetKind::Erc20);
    assert_eq!(action.inputs[1].asset.address, Some(usdbc()));
    assert_eq!(action.inputs[1].amount.kind, AmountKind::Max);
    assert_eq!(action.recipient, recipient());

    // Stable flag must not leak.
    let json = serde_json::to_value(envelope).expect("envelope serialises");
    let json_str = json.to_string();
    assert!(
        !json_str.contains("stable"),
        "addLiquidity envelope must not carry `stable`, got: {json_str}"
    );
}

/// **T8: `removeLiquidityETHSupportingFeeOnTransferTokens` — native leg + FoT dialect drop**.
///
/// Maps to `Action::RemoveLiquidity` with `outputTokens[0]` = ERC20 token,
/// `outputTokens[1]` = native (no address, only kind). The "ETH" leg comes
/// out of the router's WETH unwrap, so the envelope reports it as `native`
/// rather than WETH. The V1.0.0 bundle also drops the
/// `adapter:aerodrome.fee_on_transfer` dialect — pinned here.
#[test]
fn aerodrome_remove_liquidity_eth_fot_emits_native_output_leg() {
    let mapper = load_mapper(AERO_REMOVE_LIQUIDITY_ETH_FOT);
    let ctx = Ctx::new();

    let decoded = aerodrome_remove_liquidity_eth_fot_decoded(
        mapper.declarative_decoder_id(),
        usdc(),
        false,
        U256::from(1_000_000_000_000_000_000_u64), // 1e18 LP
        U256::from(900_000_u64),
        U256::from(500_000_000_000_000_000_u64), // 0.5e18 ETH min
        recipient(),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("removeLiquidityETHSupportingFeeOnTransferTokens maps");
    let envelope = &envelopes[0];
    let Action::RemoveLiquidity(action) = &envelope.action else {
        panic!(
            "expected Action::RemoveLiquidity, got {:?}",
            envelope.action
        );
    };

    assert_eq!(action.exit_mode, RemoveLiquidityExitMode::Proportional);
    assert_eq!(action.outputs.len(), 2);
    // First output: ERC20 token leg.
    assert_eq!(action.outputs[0].asset.kind, AssetKind::Erc20);
    assert_eq!(action.outputs[0].asset.address, Some(usdc()));
    assert_eq!(action.outputs[0].amount.kind, AmountKind::Min);
    // Second output: native (ETH) leg — no address field, only `native` kind.
    assert_eq!(action.outputs[1].asset.kind, AssetKind::Native);
    assert_eq!(
        action.outputs[1].asset.address, None,
        "native leg must not carry a token address"
    );
    assert_eq!(action.outputs[1].amount.kind, AmountKind::Min);
    assert_eq!(
        action.outputs[1]
            .amount
            .value
            .as_ref()
            .map(ToString::to_string),
        Some("500000000000000000".to_owned())
    );

    // FoT dialect surface — must not surface in V1.0.0.
    let json = serde_json::to_value(envelope).expect("envelope serialises");
    let json_str = json.to_string();
    assert!(
        !json_str.contains("fee_on_transfer"),
        "envelope must not carry `fee_on_transfer` in V1.0.0, got: {json_str}"
    );
    assert!(
        !json_str.contains("adapter:aerodrome"),
        "envelope must not carry `adapter:aerodrome.*` dialect, got: {json_str}"
    );

    // Evaluate-stage contract: the serialized envelope must deserialize back.
    // `inputLp` is a V2 LP token whose address is a CREATE2 result absent
    // from calldata — the bundle emits `kind: "unknown"` so `AssetRef`'s
    // address requirement does not fail-close the evaluate stage with
    // `__engine::invalid_input_json`.
    let envelope_json = serde_json::to_string(envelope).expect("envelope serialises");
    serde_json::from_str::<ActionEnvelope>(&envelope_json).unwrap_or_else(|err| {
        panic!(
            "removeLiquidity envelope must deserialize back (evaluate-stage contract); \
             got error: {err}\njson: {envelope_json}"
        )
    });
}

/// **T9: `swapExactETHForTokens` sources native input from `$.tx.value_wei`**.
///
/// The bundle binds `inputToken.amount.value = $.tx.value_wei`, not an
/// arg. `MapContext.value_wei` thus flows verbatim into the envelope. The
/// `inputToken.asset.kind = native` with no address field.
#[test]
fn aerodrome_swap_exact_eth_for_tokens_uses_tx_value_as_input_amount() {
    let mapper = load_mapper(AERO_SWAP_EXACT_ETH_FOR_TOKENS);
    // Set a non-trivial `value_wei` — 0.1 ETH = 1e17 wei.
    let ctx = Ctx::with_value("100000000000000000");
    let routes = vec![route(weth(), usdc(), false, aerodrome_default_factory())];

    let decoded = aerodrome_swap_eth_decoded(
        mapper.declarative_decoder_id(),
        U256::from(300_000_u64), // 0.3 USDC minimum
        routes,
        recipient(),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("ETH->Tokens swap maps");
    let action = unwrap_swap(&envelopes[0]);

    assert_eq!(action.input_token.asset.kind, AssetKind::Native);
    assert_eq!(
        action.input_token.asset.address, None,
        "native input must not carry an address"
    );
    assert_eq!(action.input_token.amount.kind, AmountKind::Exact);
    assert_eq!(
        action
            .input_token
            .amount
            .value
            .as_ref()
            .map(ToString::to_string),
        Some("100000000000000000".to_owned()),
        "input amount must echo $.tx.value_wei verbatim"
    );

    // Output side — `routes[-1][1]` should be USDC.
    assert_eq!(action.output_token.asset.kind, AssetKind::Erc20);
    assert_eq!(action.output_token.asset.address, Some(usdc()));
    assert_eq!(action.output_token.amount.kind, AmountKind::Min);
}

/// **T10: zero `amountIn` is passed through verbatim**.
///
/// The declarative interpreter is observability-only — validation rules
/// like "forbid zero amount" belong to the Cedar policy layer. This test
/// pins that `amountIn=0` produces an envelope with `value="0"` exactly,
/// instead of being silently rejected or substituted.
#[test]
fn aerodrome_swap_zero_amount_in_emits_zero_envelope() {
    let mapper = load_mapper(AERO_SWAP_EXACT_TOKENS_FOR_TOKENS);
    let ctx = Ctx::new();
    let routes = vec![route(usdc(), usdt(), false, aerodrome_default_factory())];

    let decoded = aerodrome_swap_decoded(
        mapper.declarative_decoder_id(),
        U256::ZERO,
        U256::ZERO,
        routes,
        recipient(),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("zero-amount swap should still map (validation is policy concern)");
    let action = unwrap_swap(&envelopes[0]);

    assert_eq!(action.input_token.amount.kind, AmountKind::Exact);
    assert_eq!(
        action
            .input_token
            .amount
            .value
            .as_ref()
            .map(ToString::to_string),
        Some("0".to_owned()),
        "zero amountIn must surface as exact \"0\" decimal string"
    );
    assert_eq!(action.output_token.amount.kind, AmountKind::Min);
    assert_eq!(
        action
            .output_token
            .amount
            .value
            .as_ref()
            .map(ToString::to_string),
        Some("0".to_owned())
    );
}

/// **T11: `addLiquidity` envelope survives the evaluate-stage pipeline**.
///
/// Regression for the `__engine::invalid_input_json` false-`fail`. The
/// declarative route serializes envelopes; the evaluate entrypoint then
/// deserializes them and lowers each to a Cedar request. `AssetRef`'s
/// custom `Deserialize` runs on the way back in and rejects an `erc20`
/// asset with no address — so an `outputLp` whose address the bundle
/// cannot emit (the V2 LP token is a CREATE2 result, absent from calldata)
/// broke the round-trip and the engine fail-closed to a verdict of `fail`.
///
/// This walks the full contract: route → serialize → **deserialize** →
/// lower → evaluate. With the bundle emitting `outputLp.asset.kind:
/// "unknown"` the asset carries no address requirement and the pipeline
/// completes to a real `Verdict`.
#[test]
fn aerodrome_add_liquidity_envelope_survives_evaluate_pipeline() {
    let mapper = load_mapper(AERO_ADD_LIQUIDITY);
    let ctx = Ctx::new();

    let decoded = aerodrome_add_liquidity_decoded(
        mapper.declarative_decoder_id(),
        usdc(),
        usdbc(),
        false,
        U256::from(1_000_000_u64),
        U256::from(1_000_000_u64),
        recipient(),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("addLiquidity maps");
    assert_eq!(envelopes.len(), 1);

    // Stage 1 — the declarative route serializes the envelope, the evaluate
    // entrypoint deserializes it back. `AssetRef`'s custom `Deserialize`
    // runs here: an `erc20` asset with no address is rejected, surfacing as
    // `__engine::invalid_input_json`.
    let json = serde_json::to_string(&envelopes[0]).expect("envelope serialises");
    let roundtripped = serde_json::from_str::<ActionEnvelope>(&json).unwrap_or_else(|err| {
        panic!(
            "addLiquidity envelope must deserialize back (evaluate-stage contract); \
             got error: {err}\njson: {json}"
        )
    });

    // Stage 2 — lower to a Cedar policy request.
    let request = policy_request_from_envelope(
        &roundtripped,
        &ctx.from,
        &ctx.to,
        &ctx.value,
        8453,
        1_700_000_000,
    )
    .expect("add_liquidity envelope must lower to a policy request");

    // Stage 3 — evaluate against an empty policy set. The `unknown`-kind
    // outputLp asset must survive the Cedar schema; with no policies the
    // verdict is Pass.
    let engine = PolicyEngineBuilder::new()
        .build()
        .expect("policy engine builds");
    let verdict = engine
        .evaluate(
            &request.principal,
            &request.action,
            &request.resource,
            &request.entities,
            &request.context,
        )
        .expect("add_liquidity request must evaluate");
    assert_eq!(verdict, Verdict::Pass);
}

// ───────────────────────────────────────────────────────────────────────────
// T12–T15 — user policy evaluation over the `add_liquidity` envelope.
//
// T11 proved the envelope survives the evaluate pipeline to a real Verdict
// under an *empty* policy set. T12–T15 pin that the Verdict actually tracks
// user Cedar policies. The three policies are the ones documented for the
// dashboard dynamic-add walkthrough; each gates a field a static-only
// analysis can read on an `add_liquidity`:
//
//   * P1 `recipient` — a drainer defence (LP minted to a foreign address).
//   * P2 `outputLp.asset.kind` — the `unknown` placeholder this engagement
//     introduced for the un-derivable V2 LP token.
//   * P3 `pool.address` — the `0x0` placeholder for the pool address, which
//     Aerodrome V2 `addLiquidity` calldata never carries (the pool is
//     `PoolFactory.getPool(tokenA, tokenB, stable)`).
//
// Production code is unchanged — test-only coverage.
// ───────────────────────────────────────────────────────────────────────────

/// **P1** — LP-recipient self-guard. An `addLiquidity` whose LP token is
/// minted to an address other than the signer is a classic drainer shape.
/// `principal.address` is the tx signer; a self-recipient tx passes.
const POLICY_RECIPIENT_SELF_GUARD: &str = r#"@id("user/add-liquidity-recipient-self")
@severity("deny")
@reason("LP token recipient differs from the signing wallet")
forbid (
  principal,
  action == Action::"add_liquidity",
  resource
) when {
  context.recipient != principal.address
};
"#;

/// **P2** — unknown LP-token warning. The V2 LP token address is a CREATE2
/// result absent from calldata, so the bundle emits `outputLp.asset.kind:
/// "unknown"`. A cautious user routes that to a warn.
const POLICY_UNKNOWN_LP_WARN: &str = r#"@id("user/add-liquidity-unknown-lp")
@severity("warn")
@reason("The LP token to be received could not be statically identified")
forbid (
  principal,
  action == Action::"add_liquidity",
  resource
) when {
  context.outputLp.asset.kind == "unknown"
};
"#;

/// **P3** — unidentified-pool block. Aerodrome V2 `addLiquidity` carries no
/// pool address in calldata, so the bundle emits the zero address as a
/// placeholder. A conservative user denies liquidity provision to a pool the
/// analyzer cannot pin down.
const POLICY_UNIDENTIFIED_POOL_DENY: &str = r#"@id("user/add-liquidity-unidentified-pool")
@severity("deny")
@reason("The target pool could not be statically identified (pool.address unresolved)")
forbid (
  principal,
  action == Action::"add_liquidity",
  resource
) when {
  context.pool.address == "0x0000000000000000000000000000000000000000"
};
"#;

/// Route an Aerodrome V2 `addLiquidity` call through the declarative mapper,
/// lower it to a Cedar request (mirroring T11's evaluate-stage serialize →
/// deserialize → lower contract), install `policy`, and evaluate.
/// `recipient_addr` becomes the `add_liquidity` context's `recipient`; the
/// signer (`principal`) is always `signer()`.
fn evaluate_add_liquidity(recipient_addr: Address, policy: &str) -> Verdict {
    let mapper = load_mapper(AERO_ADD_LIQUIDITY);
    let ctx = Ctx::new();

    let decoded = aerodrome_add_liquidity_decoded(
        mapper.declarative_decoder_id(),
        usdc(),
        usdbc(),
        false,
        U256::from(1_000_000_u64),
        U256::from(1_000_000_u64),
        recipient_addr,
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("addLiquidity maps");
    assert_eq!(envelopes.len(), 1);

    let json = serde_json::to_string(&envelopes[0]).expect("envelope serialises");
    let roundtripped = serde_json::from_str::<ActionEnvelope>(&json)
        .expect("addLiquidity envelope must deserialize back");

    let request = policy_request_from_envelope(
        &roundtripped,
        &ctx.from,
        &ctx.to,
        &ctx.value,
        8453,
        1_700_000_000,
    )
    .expect("add_liquidity envelope must lower to a policy request");

    let engine = PolicyEngineBuilder::new()
        .add_text(policy)
        .build()
        .expect("policy engine builds");
    engine
        .evaluate(
            &request.principal,
            &request.action,
            &request.resource,
            &request.entities,
            &request.context,
        )
        .expect("add_liquidity request must evaluate")
}

/// **T12 (P1 / pass)** — recipient equals the signer, so the self-guard's
/// `when` clause is false and the deny does not fire.
#[test]
fn aerodrome_add_liquidity_recipient_self_guard_passes_for_self_recipient() {
    let verdict = evaluate_add_liquidity(signer(), POLICY_RECIPIENT_SELF_GUARD);
    assert_eq!(verdict, Verdict::Pass);
}

/// **T13 (P1 / fail)** — recipient is a foreign address, so the self-guard
/// fires and the verdict is a deny-severity fail.
#[test]
fn aerodrome_add_liquidity_recipient_self_guard_fails_for_foreign_recipient() {
    let verdict = evaluate_add_liquidity(recipient(), POLICY_RECIPIENT_SELF_GUARD);
    match verdict {
        Verdict::Fail(matched) => assert!(
            matched
                .iter()
                .any(|policy| policy.policy_id.contains("add-liquidity-recipient-self")),
            "expected the recipient self-guard to match, got {matched:?}"
        ),
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}

/// **T14 (P2 / warn)** — the V2 LP token is emitted as `kind: "unknown"`, so
/// the unknown-LP policy matches at warn severity.
#[test]
fn aerodrome_add_liquidity_unknown_lp_policy_warns() {
    let verdict = evaluate_add_liquidity(signer(), POLICY_UNKNOWN_LP_WARN);
    match verdict {
        Verdict::Warn(matched) => assert!(
            matched
                .iter()
                .any(|policy| policy.policy_id.contains("add-liquidity-unknown-lp")),
            "expected the unknown-LP policy to match, got {matched:?}"
        ),
        other => panic!("expected Verdict::Warn, got {other:?}"),
    }
}

/// **T15 (P3 / fail)** — the V2 `addLiquidity` bundle emits the zero address
/// for `pool.address`, so the unidentified-pool policy fires as a deny.
#[test]
fn aerodrome_add_liquidity_unidentified_pool_policy_denies() {
    let verdict = evaluate_add_liquidity(signer(), POLICY_UNIDENTIFIED_POOL_DENY);
    match verdict {
        Verdict::Fail(matched) => assert!(
            matched
                .iter()
                .any(|policy| policy.policy_id.contains("add-liquidity-unidentified-pool")),
            "expected the unidentified-pool policy to match, got {matched:?}"
        ),
        other => panic!("expected Verdict::Fail, got {other:?}"),
    }
}
