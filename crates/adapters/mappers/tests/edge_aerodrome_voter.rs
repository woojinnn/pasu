//! Aerodrome Voter edge case integration tests (Phase 8 Round 6 — A-TEST-AERO-VOTER).
//!
//! Exercises the Voter bundle family (`vote`, `reset`, `poke`, `claimFees`,
//! `claimBribes`) end-to-end through [`DeclarativeMapper`]. Each test loads the
//! production registry manifest via `include_str!` (read-only) and crafts a
//! [`DecodedCall`] that mimics what `bridge.rs` produces after the abi-resolver
//! has decoded the calldata, then asserts the resulting envelope against the
//! Round 3 builder semantics (`build_gauge_vote_envelope`,
//! `build_claim_rewards_envelope`).
//!
//! Round 3 / Round 4 / Round 5a wiring under test:
//!   * `build_gauge_vote_envelope`: pools/weights parallel-array length check,
//!     `kind` literal propagation (`vote` / `reset` / `poke`).
//!   * `build_claim_rewards_envelope`: `source.address` + `source.label` merge
//!     into `SourceRef`; `rewardTokens[0]` first-element fan-out for
//!     `claimFees` / `claimBribes` (Round 4.4 한계 — array-of-array `tokens[][]`
//!     의 첫 element 만 emit; nested fan-out 의 generalisation 은 후속 작업).
//!
//! 본 file 의 목적 = edge case 만 — happy-path equivalence test 는 이미
//! `crates/adapters/mappers/src/declarative/single_emit.rs` 의 unit test 가
//! 다룸.

use std::str::FromStr as _;

use abi_resolver::{DecodedArg, DecodedCall, DecodedValue};
use alloy_primitives::U256;
use mappers::declarative::{types::AdapterFunctionBundle, DeclarativeMapper};
use mappers::mapper::{MapContext, Mapper, MapperError};
use mappers::EmptyTokenRegistry;
use policy_engine::action::misc::{ClaimRewardsAction, GaugeVoteAction, GaugeVoteKind, SourceRef};
use policy_engine::action::{Action, Address, AssetKind, DecimalString};

// ───────────────────────────────────────────────────────────────────────────
// Bundle fixtures — production registry manifests (read-only `include_str!`).
// Path resolves relative to this file:
// `crates/adapters/mappers/tests/edge_aerodrome_voter.rs` →
// `../../../../registry/manifests/aerodrome/voter/...`
// (4 `..` to climb out of `crates/adapters/mappers/tests/`).
// ───────────────────────────────────────────────────────────────────────────

const VOTE_BUNDLE: &str =
    include_str!("../../../../registry/manifests/aerodrome/voter/vote@1.0.0.json");
const RESET_BUNDLE: &str =
    include_str!("../../../../registry/manifests/aerodrome/voter/reset@1.0.0.json");
const POKE_BUNDLE: &str =
    include_str!("../../../../registry/manifests/aerodrome/voter/poke@1.0.0.json");
const CLAIM_FEES_BUNDLE: &str =
    include_str!("../../../../registry/manifests/aerodrome/voter/claimFees@1.0.0.json");

// ───────────────────────────────────────────────────────────────────────────
// Address fixtures — Base mainnet checksums lowercased (Address::from_str
// normalises on parse, so the canonical form is the lowercase 0x-prefix).
// ───────────────────────────────────────────────────────────────────────────

/// Aerodrome Voter on Base — matches the `match.to` field in every Voter bundle.
fn aero_voter() -> Address {
    Address::from_str("0x16613524e02ad97edfef371bc883f2f5d6c480a5").unwrap()
}

/// USDC/WETH AeroV2 pool (placeholder — mainnet checksum not asserted, the
/// interpreter does not enforce pool existence).
fn pool_usdc_eth() -> Address {
    Address::from_str("0xcdac0d6c6c59727a65f871236188350531885c43").unwrap()
}

fn pool(label: u8) -> Address {
    let suffix = format!("{label:02x}");
    Address::from_str(&format!("0x{}{}", "0".repeat(38), suffix)).unwrap()
}

fn user() -> Address {
    Address::from_str("0x00000000000000000000000000000000000000aa").unwrap()
}

fn fee_voter_a() -> Address {
    Address::from_str("0x1111111111111111111111111111111111111111").unwrap()
}

fn fee_voter_b() -> Address {
    Address::from_str("0x2222222222222222222222222222222222222222").unwrap()
}

fn usdc() -> Address {
    Address::from_str("0x833589fcd6edb6e08f4c7c32d4f71b54bda02913").unwrap()
}

fn weth() -> Address {
    Address::from_str("0x4200000000000000000000000000000000000006").unwrap()
}

fn aero() -> Address {
    Address::from_str("0x940181a94a35a4569e4529a3cdfb74e38fd98631").unwrap()
}

// ───────────────────────────────────────────────────────────────────────────
// MapContext helper — Base chain (8453) so the bundle's `match.chain_ids`
// matches; the declarative interpreter does not re-check this match (the
// dispatcher upstream owns that check) but using the realistic value keeps
// the test reflective of production wiring.
// ───────────────────────────────────────────────────────────────────────────

struct Ctx {
    registry: EmptyTokenRegistry,
    from: Address,
    to: Address,
    value: DecimalString,
}

impl Ctx {
    fn new() -> Self {
        Self {
            registry: EmptyTokenRegistry,
            from: user(),
            to: aero_voter(),
            value: DecimalString::from_str("0").unwrap(),
        }
    }

    fn map_ctx(&self) -> MapContext<'_> {
        MapContext::new(
            8453,
            &self.from,
            &self.to,
            &self.value,
            Some(1_700_000_000),
            &self.registry,
        )
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Decoded-call builders. The Voter ABIs `vote(uint256, address[], uint256[])`,
// `reset(uint256)`, `poke(uint256)`, `claimFees(address[], address[][], uint256)`,
// `claimBribes(address[], address[][], uint256)` are already decoded upstream
// by abi-resolver — these helpers fabricate the `DecodedCall` shape the
// declarative mapper consumes (top-level args, no flattening here because the
// Voter ABIs have no tuple wrappers).
// ───────────────────────────────────────────────────────────────────────────

fn make_vote_call(
    mapper: &DeclarativeMapper,
    token_id: U256,
    pools: Vec<Address>,
    weights: Vec<U256>,
) -> DecodedCall {
    DecodedCall {
        decoder_id: mapper.declarative_decoder_id(),
        function_signature: "vote(uint256,address[],uint256[])".into(),
        args: vec![
            DecodedArg {
                name: "_tokenId".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(token_id),
            },
            DecodedArg {
                name: "_poolVote".into(),
                abi_type: "address[]".into(),
                value: DecodedValue::Array(pools.into_iter().map(DecodedValue::Address).collect()),
            },
            DecodedArg {
                name: "_weights".into(),
                abi_type: "uint256[]".into(),
                value: DecodedValue::Array(weights.into_iter().map(DecodedValue::Uint).collect()),
            },
        ],
        nested: vec![],
    }
}

fn make_reset_or_poke_call(mapper: &DeclarativeMapper, sig: &str, token_id: U256) -> DecodedCall {
    DecodedCall {
        decoder_id: mapper.declarative_decoder_id(),
        function_signature: sig.into(),
        args: vec![DecodedArg {
            name: "_tokenId".into(),
            abi_type: "uint256".into(),
            value: DecodedValue::Uint(token_id),
        }],
        nested: vec![],
    }
}

/// `claimFees(address[] fees, address[][] tokens, uint256 tokenId)`.
fn make_claim_fees_call(
    mapper: &DeclarativeMapper,
    fees: Vec<Address>,
    tokens: Vec<Vec<Address>>,
    token_id: U256,
) -> DecodedCall {
    DecodedCall {
        decoder_id: mapper.declarative_decoder_id(),
        function_signature: "claimFees(address[],address[][],uint256)".into(),
        args: vec![
            DecodedArg {
                name: "_fees".into(),
                abi_type: "address[]".into(),
                value: DecodedValue::Array(fees.into_iter().map(DecodedValue::Address).collect()),
            },
            DecodedArg {
                name: "_tokens".into(),
                abi_type: "address[][]".into(),
                value: DecodedValue::Array(
                    tokens
                        .into_iter()
                        .map(|inner| {
                            DecodedValue::Array(
                                inner.into_iter().map(DecodedValue::Address).collect(),
                            )
                        })
                        .collect(),
                ),
            },
            DecodedArg {
                name: "_tokenId".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(token_id),
            },
        ],
        nested: vec![],
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Bundle loader helpers.
// ───────────────────────────────────────────────────────────────────────────

fn load_mapper(bundle_json: &str) -> DeclarativeMapper {
    let bundle: AdapterFunctionBundle = serde_json::from_str(bundle_json).expect("bundle parses");
    DeclarativeMapper::new(bundle)
}

fn unwrap_gauge_vote(envelopes: &[policy_engine::ActionEnvelope]) -> &GaugeVoteAction {
    assert_eq!(
        envelopes.len(),
        1,
        "single envelope expected, got {}",
        envelopes.len()
    );
    match &envelopes[0].action {
        Action::GaugeVote(a) => a,
        other => panic!("expected Action::GaugeVote, got {other:?}"),
    }
}

fn unwrap_claim_rewards(envelopes: &[policy_engine::ActionEnvelope]) -> &ClaimRewardsAction {
    assert_eq!(
        envelopes.len(),
        1,
        "single envelope expected, got {}",
        envelopes.len()
    );
    match &envelopes[0].action {
        Action::ClaimRewards(a) => a,
        other => panic!("expected Action::ClaimRewards, got {other:?}"),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Tests
// ───────────────────────────────────────────────────────────────────────────

/// **V1: vote with single pool → kind=vote, pools=[1], weights=[10000]**.
///
/// Smallest happy-path vote. Confirms the bundle's `kind: { literal: "vote" }`
/// propagates and that `pools` / `weights` are passed through from
/// `$.args.pools` / `$.args.weights` verbatim.
#[test]
fn vote_with_single_pool() {
    let mapper = load_mapper(VOTE_BUNDLE);
    let ctx = Ctx::new();

    let call = make_vote_call(
        &mapper,
        U256::from(1_u64),
        vec![pool_usdc_eth()],
        vec![U256::from(10_000_u64)],
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &call)
        .expect("single-pool vote maps");
    let action = unwrap_gauge_vote(&envelopes);

    assert_eq!(action.voter, aero_voter());
    assert_eq!(action.token_id.as_ref().unwrap().to_string(), "1");
    assert_eq!(action.pools.len(), 1);
    assert_eq!(action.weights.len(), 1);
    assert_eq!(action.pools[0], pool_usdc_eth());
    assert_eq!(action.weights[0].to_string(), "10000");
    assert_eq!(action.kind, Some(GaugeVoteKind::Vote));
    assert!(action.validity.is_none());
}

/// **V2: vote with 5 pools → length / index parity preserved**.
///
/// Wider fan-out exercises the JSON-tree array build in `set_nested`. The
/// declarative interpreter does NOT enforce a weight-sum invariant (e.g.
/// `sum == 10000`); that's a Cedar policy concern. Only the parallel-array
/// length is checked by the Rust builder.
#[test]
fn vote_with_5_pools_preserves_order_and_length() {
    let mapper = load_mapper(VOTE_BUNDLE);
    let ctx = Ctx::new();

    let pools = vec![pool(0x01), pool(0x02), pool(0x03), pool(0x04), pool(0x05)];
    // Arbitrary sum — explicitly NOT 10_000 to confirm the interpreter does
    // not enforce a normalised total.
    let weights: Vec<U256> = vec![1_u64, 2_u64, 3_u64, 4_u64, 5_u64]
        .into_iter()
        .map(U256::from)
        .collect();

    let call = make_vote_call(&mapper, U256::from(42_u64), pools.clone(), weights.clone());

    let envelopes = mapper.map(&ctx.map_ctx(), &call).expect("5-pool vote maps");
    let action = unwrap_gauge_vote(&envelopes);

    assert_eq!(action.pools.len(), 5);
    assert_eq!(action.weights.len(), 5);
    for (i, p) in pools.iter().enumerate() {
        assert_eq!(&action.pools[i], p, "pool[{i}] preserved");
    }
    for (i, w) in weights.iter().enumerate() {
        assert_eq!(
            action.weights[i].to_string(),
            w.to_string(),
            "weight[{i}] preserved"
        );
    }
    assert_eq!(action.kind, Some(GaugeVoteKind::Vote));
}

/// **V3: weight = max u256 → DecimalString round-trips without precision loss**.
///
/// The interpreter normalises uint256 → decimal string via
/// `u256_to_decimal_string`. `2^256 - 1` exceeds JS `Number.MAX_SAFE_INTEGER`
/// (`2^53 - 1`) by ~23 orders of magnitude, so the string representation must
/// be preserved end-to-end. The declarative interpreter does not validate
/// weight magnitude — Cedar's `forbid-zero-weight-sum` / cap rules are the
/// downstream check.
#[test]
fn vote_weight_max_uint256_round_trips() {
    let mapper = load_mapper(VOTE_BUNDLE);
    let ctx = Ctx::new();

    let call = make_vote_call(
        &mapper,
        U256::from(1_u64),
        vec![pool_usdc_eth()],
        vec![U256::MAX],
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &call)
        .expect("max-weight vote maps");
    let action = unwrap_gauge_vote(&envelopes);

    let expected_max =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    assert_eq!(action.weights.len(), 1);
    assert_eq!(
        action.weights[0].to_string(),
        expected_max,
        "max uint256 weight preserved as decimal string"
    );
}

/// **V4: weight = 0 with kind=vote → envelope emitted, Cedar policy decides**.
///
/// `[0]` is a valid emit at the mapping layer (`build_gauge_vote_envelope`
/// only checks `pools.len() == weights.len()`). The downstream Cedar default
/// policy `forbid-zero-weight-sum` (Round 1 A-B-VARIANTS) is expected to
/// `forbid` this envelope — but that's a Cedar-side test, not this file's
/// concern. Here we only verify mapping passes.
#[test]
fn vote_weight_zero_with_kind_vote_maps_pass() {
    let mapper = load_mapper(VOTE_BUNDLE);
    let ctx = Ctx::new();

    let call = make_vote_call(
        &mapper,
        U256::from(7_u64),
        vec![pool_usdc_eth()],
        vec![U256::ZERO],
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &call)
        .expect("zero-weight vote maps");
    let action = unwrap_gauge_vote(&envelopes);

    assert_eq!(action.weights.len(), 1);
    assert_eq!(action.weights[0].to_string(), "0");
    assert_eq!(action.kind, Some(GaugeVoteKind::Vote));
    // Mapping layer pass — Cedar verdict (forbid) is asserted elsewhere.
}

/// **V5: reset → pools = [], weights = [], kind = reset**.
///
/// The `reset@1.0.0.json` bundle binds `pools` / `weights` as `literal: []`
/// (NOT from `$.args` — `reset(uint256)` has only `tokenId`). Confirms the
/// literal-array path through `evaluate_value_expr` lands as empty `Vec`s.
#[test]
fn reset_emits_empty_pools_and_weights() {
    let mapper = load_mapper(RESET_BUNDLE);
    let ctx = Ctx::new();

    let call = make_reset_or_poke_call(&mapper, "reset(uint256)", U256::from(99_u64));

    let envelopes = mapper.map(&ctx.map_ctx(), &call).expect("reset maps");
    let action = unwrap_gauge_vote(&envelopes);

    assert_eq!(action.voter, aero_voter());
    assert_eq!(action.token_id.as_ref().unwrap().to_string(), "99");
    assert!(action.pools.is_empty(), "reset must emit empty pools");
    assert!(action.weights.is_empty(), "reset must emit empty weights");
    assert_eq!(action.kind, Some(GaugeVoteKind::Reset));
}

/// **V6: poke → pools = [], weights = [], kind = poke**.
///
/// Same shape as reset but kind discriminator differs. `poke` semantically
/// refreshes an existing vote without changing weights — the on-chain ABI
/// carries only `tokenId`, so the bundle uses `literal: []` for the
/// schema-required `pools` / `weights` fields.
#[test]
fn poke_emits_empty_pools_with_poke_kind() {
    let mapper = load_mapper(POKE_BUNDLE);
    let ctx = Ctx::new();

    let call = make_reset_or_poke_call(&mapper, "poke(uint256)", U256::from(42_u64));

    let envelopes = mapper.map(&ctx.map_ctx(), &call).expect("poke maps");
    let action = unwrap_gauge_vote(&envelopes);

    assert_eq!(action.token_id.as_ref().unwrap().to_string(), "42");
    assert!(action.pools.is_empty());
    assert!(action.weights.is_empty());
    assert_eq!(action.kind, Some(GaugeVoteKind::Poke));
}

/// **V7: pools.len() != weights.len() → MapperError::Internal**.
///
/// `vote(tokenId, [a, b], [100])` — 2 pools but only 1 weight. The builder's
/// `if pools.len() != weights.len() { return Err(Internal(...)) }` check
/// fires. This is the only schema-level integrity check the declarative
/// interpreter enforces for `gauge_vote`; everything else (weight cap, sum
/// rules, non-empty pools when kind=vote) is the Cedar engine's responsibility.
#[test]
fn vote_pools_weights_length_mismatch_errors() {
    let mapper = load_mapper(VOTE_BUNDLE);
    let ctx = Ctx::new();

    let call = make_vote_call(
        &mapper,
        U256::from(1_u64),
        vec![pool(0x01), pool(0x02)],
        vec![U256::from(100_u64)], // length 1, not 2
    );

    let err = mapper
        .map(&ctx.map_ctx(), &call)
        .expect_err("length mismatch must error");
    let msg = err.to_string();
    assert!(
        matches!(err, MapperError::Internal(_)),
        "expected MapperError::Internal, got {err:?}"
    );
    assert!(
        msg.contains("pools.len()=2 != weights.len()=1"),
        "error must reference parallel-array length mismatch, got {msg:?}"
    );
}

/// **V8: claimFees → rewardTokens[0] = tokens[0][0]; source = (voter, label)**.
///
/// `claimFees(fees=[A, B], tokens=[[USDC, WETH], [AERO]], tokenId=1)`. The
/// Round 4.4 bundle emits only the **first element** of the nested
/// `tokens[][]` array — `rewardTokens[0].address = $.args.tokens[0][0]` →
/// USDC. Generalised fan-out (one envelope per (fee, token) pair) is a
/// follow-up — this test pins the limited single-emit semantics so future
/// expansions don't silently change behaviour.
///
/// Source merge: bundle's `source.address` + `source.label` dot-paths land in
/// the JSON tree as `{ source: { address, label } }`, which
/// `read_optional_source_ref` rehydrates into the `SourceRef` field.
#[test]
fn claim_fees_emits_first_token_only_with_source() {
    let mapper = load_mapper(CLAIM_FEES_BUNDLE);
    let ctx = Ctx::new();

    let call = make_claim_fees_call(
        &mapper,
        vec![fee_voter_a(), fee_voter_b()],
        vec![vec![usdc(), weth()], vec![aero()]],
        U256::from(1_u64),
    );

    let envelopes = mapper.map(&ctx.map_ctx(), &call).expect("claimFees maps");
    let action = unwrap_claim_rewards(&envelopes);

    // Source — bundle literal address (Voter) + "Aerodrome Voter (Fees)" label.
    let source = action
        .source
        .as_ref()
        .expect("source populated by claimFees bundle");
    assert_eq!(
        source,
        &SourceRef {
            address: Some(aero_voter()),
            label: Some("Aerodrome Voter (Fees)".to_owned()),
        }
    );

    // from / recipient — both bound to `$.tx.from` (no `recipient` argument
    // in the ABI; the Voter forwards to msg.sender).
    assert_eq!(action.from, user());
    assert_eq!(action.recipient, user());

    // tokenId from calldata.
    assert_eq!(
        action.token_id.as_ref().map(ToString::to_string),
        Some("1".to_owned())
    );

    // rewardTokens — only the **first** of `tokens[0][0]` is captured
    // (USDC), reflecting the Round 4.4 single-emit fan-out 한계.
    let reward_tokens = action
        .reward_tokens
        .as_ref()
        .expect("rewardTokens populated");
    assert_eq!(
        reward_tokens.len(),
        1,
        "Round 4.4 bundle emits only tokens[0][0]; full fan-out 는 후속 작업"
    );
    assert_eq!(reward_tokens[0].kind, AssetKind::Erc20);
    assert_eq!(reward_tokens[0].address, Some(usdc()));

    // Bundle does not set maxAmounts — the field should be `None`.
    assert!(action.max_amounts.is_none());
}
