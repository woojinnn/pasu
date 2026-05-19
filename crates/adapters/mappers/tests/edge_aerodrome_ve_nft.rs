//! Aerodrome VotingEscrow (veAERO) edge case tests — Phase 8 Round 6
//! (A-TEST-AERO-VE-NFT).
//!
//! Exercises the six VotingEscrow bundles (`createLock`, `createLockFor`,
//! `increaseAmount`, `increaseUnlockTime`, `merge`, `split`) end-to-end
//! through the `DeclarativeMapper`. Each test builds a `DecodedCall` that
//! mirrors what `bridge.rs` produces post-decode and asserts the resulting
//! `ActionEnvelope` payload from
//! `single_emit::build_lock_create_envelope` / `build_lock_increase_envelope`
//! / `build_lock_manage_envelope`.
//!
//! Coverage focus (plan §6.6):
//!
//!   * **Max-duration lock** — 4 yr (`126_144_000` sec) at the upper-bound
//!     ve(3,3) lock window (Aerodrome `VotingEscrow.sol::MAXTIME`,
//!     <https://github.com/aerodrome-finance/contracts/blob/main/contracts/VotingEscrow.sol>).
//!     The interpreter is observability-only — it does not clamp.
//!   * **Zero-duration lock** — `lockDuration = 0`. The DSL builder accepts
//!     it (no policy check); Cedar default `forbid-zero-lock-duration`
//!     downstream is responsible for forbidding.
//!   * **createLockFor recipient override** — the second variant binds
//!     `recipient` to `$.args.to` rather than `$.tx.from`. Verifies the
//!     non-sender path is honoured.
//!   * **kind discriminator on increase / manage** — `Amount` vs
//!     `UnlockTime`, `Merge` vs `Split` flow through `read_optional_enum`.
//!   * **Self-merge (from == to)** — builder emits pass; Cedar default
//!     `forbid-self-merge` is the downstream gate.
//!   * **Split ratio = 0 and > 1e18** — splitRatio is a `DecimalString` —
//!     no numeric clamp at the interpreter layer. Both values flow through
//!     verbatim (Cedar / ratio-bounds policy is downstream).
//!
//! Tests are read-only on production code — bundle manifests are loaded via
//! `include_str!` from `registry/manifests/aerodrome/voting-escrow/`.

use std::str::FromStr as _;

use abi_resolver::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
use alloy_primitives::U256;
use mappers::declarative::{types::AdapterFunctionBundle, DeclarativeMapper};
use mappers::mapper::{MapContext, Mapper};
use mappers::EmptyTokenRegistry;
use policy_engine::action::misc::{LockIncreaseKind, LockManageKind};
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountKind, AssetKind, DecimalString,
};

// ───────────────────────────────────────────────────────────────────────────
// Bundle fixtures — loaded directly from the registry manifests (read-only).
// ───────────────────────────────────────────────────────────────────────────

const CREATE_LOCK_BUNDLE: &str = include_str!(
    "../../../../registry/manifests/aerodrome/voting-escrow/createLock@1.0.0.json"
);
const CREATE_LOCK_FOR_BUNDLE: &str = include_str!(
    "../../../../registry/manifests/aerodrome/voting-escrow/createLockFor@1.0.0.json"
);
const INCREASE_AMOUNT_BUNDLE: &str = include_str!(
    "../../../../registry/manifests/aerodrome/voting-escrow/increaseAmount@1.0.0.json"
);
const INCREASE_UNLOCK_TIME_BUNDLE: &str = include_str!(
    "../../../../registry/manifests/aerodrome/voting-escrow/increaseUnlockTime@1.0.0.json"
);
const MERGE_BUNDLE: &str = include_str!(
    "../../../../registry/manifests/aerodrome/voting-escrow/merge@1.0.0.json"
);
const SPLIT_BUNDLE: &str = include_str!(
    "../../../../registry/manifests/aerodrome/voting-escrow/split@1.0.0.json"
);

// ───────────────────────────────────────────────────────────────────────────
// Address fixtures — Base mainnet canonical addresses.
//
// `Address::from_str` lowercases on parse, so use lowercase here to match
// the form returned by `to_string()` for assertion comparisons.
// ───────────────────────────────────────────────────────────────────────────

/// Aerodrome `VotingEscrow` on Base mainnet — matches every voting-escrow
/// bundle's `match.to` literal (`0xeBf418Fe2512e7E6bd9b87a8F0f294aCDC67e6B4`).
fn voting_escrow() -> Address {
    Address::from_str("0xebf418fe2512e7e6bd9b87a8f0f294acdc67e6b4").unwrap()
}

/// Aerodrome `AERO` ERC20 — bundle literal
/// (`0x940181a94A35A4569E4529A3CDfB74e38FD98631`).
fn aero_token() -> Address {
    Address::from_str("0x940181a94a35a4569e4529a3cdfb74e38fd98631").unwrap()
}

/// Tx sender — `$.tx.from` in the createLock bundle.
fn tx_from() -> Address {
    Address::from_str("0x00000000000000000000000000000000000000aa").unwrap()
}

/// Explicit recipient — `$.args.to` in the createLockFor bundle.
fn explicit_recipient() -> Address {
    Address::from_str("0x3333333333333333333333333333333333333333").unwrap()
}

// ───────────────────────────────────────────────────────────────────────────
// `MapContext` helper — chain id = 8453 (Base mainnet, where the bundles
// match), `to` = VotingEscrow, `value_wei` = 0.
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
            from: tx_from(),
            to: voting_escrow(),
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
// `DecodedCall` builders. Each mirrors the post-decode shape `bridge.rs`
// hands to the declarative mapper for the matching VotingEscrow function.
// ───────────────────────────────────────────────────────────────────────────

fn create_lock_decoded(
    decoder_id: DecoderId,
    value: U256,
    lock_duration: U256,
) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature: "createLock(uint256,uint256)".into(),
        args: vec![
            DecodedArg {
                name: "value".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(value),
            },
            DecodedArg {
                name: "lockDuration".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(lock_duration),
            },
        ],
        nested: vec![],
    }
}

fn create_lock_for_decoded(
    decoder_id: DecoderId,
    value: U256,
    lock_duration: U256,
    to: Address,
) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature: "createLockFor(uint256,uint256,address)".into(),
        args: vec![
            DecodedArg {
                name: "value".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(value),
            },
            DecodedArg {
                name: "lockDuration".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(lock_duration),
            },
            DecodedArg {
                name: "to".into(),
                abi_type: "address".into(),
                value: DecodedValue::Address(to),
            },
        ],
        nested: vec![],
    }
}

fn increase_amount_decoded(
    decoder_id: DecoderId,
    token_id: U256,
    value: U256,
) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature: "increaseAmount(uint256,uint256)".into(),
        args: vec![
            DecodedArg {
                name: "tokenId".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(token_id),
            },
            DecodedArg {
                name: "value".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(value),
            },
        ],
        nested: vec![],
    }
}

fn increase_unlock_time_decoded(
    decoder_id: DecoderId,
    token_id: U256,
    lock_duration: U256,
) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature: "increaseUnlockTime(uint256,uint256)".into(),
        args: vec![
            DecodedArg {
                name: "tokenId".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(token_id),
            },
            DecodedArg {
                name: "lockDuration".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(lock_duration),
            },
        ],
        nested: vec![],
    }
}

fn merge_decoded(decoder_id: DecoderId, from: U256, to: U256) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature: "merge(uint256,uint256)".into(),
        args: vec![
            DecodedArg {
                name: "from".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(from),
            },
            DecodedArg {
                name: "to".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(to),
            },
        ],
        nested: vec![],
    }
}

fn split_decoded(decoder_id: DecoderId, token_id: U256, ratios: U256) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature: "split(uint256,uint256)".into(),
        args: vec![
            DecodedArg {
                name: "tokenId".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(token_id),
            },
            DecodedArg {
                name: "ratios".into(),
                abi_type: "uint256".into(),
                value: DecodedValue::Uint(ratios),
            },
        ],
        nested: vec![],
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Mapper-construction helpers.
// ───────────────────────────────────────────────────────────────────────────

fn load_mapper(bundle_json: &str) -> DeclarativeMapper {
    let bundle: AdapterFunctionBundle =
        serde_json::from_str(bundle_json).expect("aerodrome ve bundle parses");
    DeclarativeMapper::new(bundle)
}

fn unwrap_lock_create(
    envelope: &ActionEnvelope,
) -> &policy_engine::action::misc::LockCreateAction {
    match &envelope.action {
        Action::LockCreate(a) => a,
        other => panic!("expected Action::LockCreate, got {other:?}"),
    }
}

fn unwrap_lock_increase(
    envelope: &ActionEnvelope,
) -> &policy_engine::action::misc::LockIncreaseAction {
    match &envelope.action {
        Action::LockIncrease(a) => a,
        other => panic!("expected Action::LockIncrease, got {other:?}"),
    }
}

fn unwrap_lock_manage(
    envelope: &ActionEnvelope,
) -> &policy_engine::action::misc::LockManageAction {
    match &envelope.action {
        Action::LockManage(a) => a,
        other => panic!("expected Action::LockManage, got {other:?}"),
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Tests
// ───────────────────────────────────────────────────────────────────────────

/// **T1: createLock at MAXTIME (4 yr)**.
///
/// Lock `1000 AERO` for the maximum lock window (`4 * 365 * 86400 =
/// 126_144_000` sec, per `VotingEscrow.sol::MAXTIME`). The interpreter is
/// observability-only — it neither clamps nor rejects boundary durations.
/// The envelope's `lockDurationSec` must round-trip verbatim, and
/// `recipient` must equal `$.tx.from` (the createLock bundle binds
/// `recipient = $.tx.from`).
#[test]
fn create_lock_at_max_duration_succeeds() {
    let mapper = load_mapper(CREATE_LOCK_BUNDLE);
    let ctx = Ctx::new();
    // 1000e18 AERO.
    let value = U256::from(1_000_u64) * U256::from(10u64).pow(U256::from(18u64));
    // MAXTIME = 4 * 365 * 86400 = 126,144,000 sec (Aerodrome veAERO).
    let max_duration = U256::from(126_144_000_u64);
    let decoded = create_lock_decoded(
        mapper.declarative_decoder_id(),
        value,
        max_duration,
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("createLock at max duration must map");
    assert_eq!(envelopes.len(), 1);
    let action = unwrap_lock_create(&envelopes[0]);

    assert_eq!(action.voting_escrow, voting_escrow());
    assert_eq!(action.asset.kind, AssetKind::Erc20);
    assert_eq!(action.asset.address, Some(aero_token()));
    assert_eq!(action.amount.kind, AmountKind::Exact);
    assert_eq!(
        action.amount.value.as_ref().map(ToString::to_string),
        Some("1000000000000000000000".to_owned())
    );
    assert_eq!(action.lock_duration_sec.to_string(), "126144000");
    // createLock binds recipient = $.tx.from.
    assert_eq!(action.recipient, tx_from());
}

/// **T2: createLock with `lockDuration = 0`** — the interpreter emits an
/// envelope; the downstream Cedar `forbid-zero-lock-duration` rule (default
/// policy) is responsible for refusal. Verifies the envelope is built and
/// `lockDurationSec` flows through as `"0"`.
#[test]
fn create_lock_zero_duration_emits_envelope() {
    let mapper = load_mapper(CREATE_LOCK_BUNDLE);
    let ctx = Ctx::new();
    let decoded = create_lock_decoded(
        mapper.declarative_decoder_id(),
        U256::from(1_u64),
        U256::ZERO,
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("zero-duration createLock must still emit (observability-only)");
    assert_eq!(envelopes.len(), 1);
    let action = unwrap_lock_create(&envelopes[0]);
    assert_eq!(action.lock_duration_sec.to_string(), "0");
    assert_eq!(
        action.amount.value.as_ref().map(ToString::to_string),
        Some("1".to_owned())
    );
}

/// **T3: createLockFor recipient = `$.args.to`** — the createLockFor bundle
/// binds `recipient` to the explicit `to` argument (not `$.tx.from`), so a
/// recipient distinct from the sender must propagate.
#[test]
fn create_lock_for_recipient_overrides_sender() {
    let mapper = load_mapper(CREATE_LOCK_FOR_BUNDLE);
    let ctx = Ctx::new();
    let value = U256::from(500_u64) * U256::from(10u64).pow(U256::from(18u64));
    let one_year = U256::from(31_536_000_u64); // 365 * 86400
    let decoded = create_lock_for_decoded(
        mapper.declarative_decoder_id(),
        value,
        one_year,
        explicit_recipient(),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("createLockFor must map");
    let action = unwrap_lock_create(&envelopes[0]);

    // recipient comes from $.args.to, NOT $.tx.from.
    assert_eq!(action.recipient, explicit_recipient());
    assert_ne!(action.recipient, ctx.from);
    assert_eq!(action.lock_duration_sec.to_string(), "31536000");
}

/// **T4: increaseAmount → `LockIncreaseKind::Amount`** — the bundle literal
/// `kind = "amount"` plus `additionalAmount.kind = "exact"` /
/// `additionalAmount.value = $.args.value`. `newLockDurationSec` is unset
/// (this is the principal-addition path, not the time-extension path).
#[test]
fn increase_amount_yields_amount_kind() {
    let mapper = load_mapper(INCREASE_AMOUNT_BUNDLE);
    let ctx = Ctx::new();
    let token_id = U256::from(42_u64);
    let additional = U256::from(500_u64) * U256::from(10u64).pow(U256::from(18u64));
    let decoded = increase_amount_decoded(
        mapper.declarative_decoder_id(),
        token_id,
        additional,
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("increaseAmount must map");
    let action = unwrap_lock_increase(&envelopes[0]);

    assert_eq!(action.kind, LockIncreaseKind::Amount);
    assert_eq!(action.token_id.to_string(), "42");
    let amount = action
        .additional_amount
        .as_ref()
        .expect("amount kind must carry additionalAmount");
    assert_eq!(amount.kind, AmountKind::Exact);
    assert_eq!(
        amount.value.as_ref().map(ToString::to_string),
        Some("500000000000000000000".to_owned())
    );
    // The time-extension field must be unset for the `amount` kind.
    assert!(action.new_lock_duration_sec.is_none());
}

/// **T5: increaseUnlockTime → `LockIncreaseKind::UnlockTime`** — the bundle
/// literal `kind = "unlock_time"` plus `newLockDurationSec = $.args.lockDuration`.
/// `additionalAmount` is unset (the principal stays the same; only time
/// extends). Uses 2 yr (`2 * 365 * 86400 = 63_072_000`).
#[test]
fn increase_unlock_time_yields_unlock_time_kind() {
    let mapper = load_mapper(INCREASE_UNLOCK_TIME_BUNDLE);
    let ctx = Ctx::new();
    let token_id = U256::from(42_u64);
    let two_years = U256::from(63_072_000_u64);
    let decoded = increase_unlock_time_decoded(
        mapper.declarative_decoder_id(),
        token_id,
        two_years,
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("increaseUnlockTime must map");
    let action = unwrap_lock_increase(&envelopes[0]);

    assert_eq!(action.kind, LockIncreaseKind::UnlockTime);
    assert_eq!(action.token_id.to_string(), "42");
    // The principal-addition field must be unset for the `unlock_time` kind.
    assert!(action.additional_amount.is_none());
    assert_eq!(
        action
            .new_lock_duration_sec
            .as_ref()
            .map(ToString::to_string),
        Some("63072000".to_owned())
    );
}

/// **T6: self-merge (`from == to`)** — Aerodrome `VotingEscrow.merge`
/// economically prohibits merging a position into itself, but the
/// declarative builder has no such check (per plan §6.6 — observability
/// only; the default policy `forbid-self-merge` is the downstream gate).
/// Verify the envelope emits with identical `from_token_id` and
/// `to_token_id` so the Cedar rule can fire on this exact shape.
#[test]
fn merge_self_merge_emits_envelope_with_equal_ids() {
    let mapper = load_mapper(MERGE_BUNDLE);
    let ctx = Ctx::new();
    let decoded = merge_decoded(
        mapper.declarative_decoder_id(),
        U256::from(5_u64),
        U256::from(5_u64),
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("self-merge must still emit (observability-only)");
    let action = unwrap_lock_manage(&envelopes[0]);

    assert_eq!(action.kind, LockManageKind::Merge);
    assert_eq!(action.from_token_id.to_string(), "5");
    assert_eq!(
        action.to_token_id.as_ref().map(ToString::to_string),
        Some("5".to_owned())
    );
    // Identical IDs — the precondition for the Cedar `forbid-self-merge` rule.
    assert_eq!(
        action.from_token_id.to_string(),
        action
            .to_token_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default()
    );
    // Split-ratio is unset for the merge variant.
    assert!(action.split_ratio.is_none());
}

/// **T7: split with `ratios = 0`** — economically nonsensical (zero-share
/// split), but the interpreter is observability-only and the
/// `DecimalString` field carries the value verbatim. The Cedar rule
/// `forbid-zero-split-ratio` would forbid downstream; here we verify the
/// envelope shape.
#[test]
fn split_zero_ratio_emits_envelope() {
    let mapper = load_mapper(SPLIT_BUNDLE);
    let ctx = Ctx::new();
    let decoded = split_decoded(
        mapper.declarative_decoder_id(),
        U256::from(5_u64),
        U256::ZERO,
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("zero-ratio split must still emit (observability-only)");
    let action = unwrap_lock_manage(&envelopes[0]);

    assert_eq!(action.kind, LockManageKind::Split);
    assert_eq!(action.from_token_id.to_string(), "5");
    // `toTokenId` is unset for the split variant (the bundle only binds
    // `fromTokenId` + `splitRatio`).
    assert!(action.to_token_id.is_none());
    assert_eq!(
        action.split_ratio.as_ref().map(ToString::to_string),
        Some("0".to_owned())
    );
}

/// **T8: split with `ratios > 1e18`** — implementation-defined upper bound.
/// Aerodrome's `VotingEscrow.split` interprets the second arg as a basis
/// for the share allocation; values above `1e18` (the standard "100 %"
/// fixed-point convention) are domain-invalid but the DSL passes them
/// through. Verify the envelope's `splitRatio` flows through verbatim,
/// keeping the decision in Cedar's hands.
#[test]
fn split_ratio_above_one_e18_emits_envelope() {
    let mapper = load_mapper(SPLIT_BUNDLE);
    let ctx = Ctx::new();
    // 1.5e18 — > 1e18 sentinel for the policy layer's ratio-bounds check.
    let oversized = U256::from(1_500_000_000_000_000_000_u128);
    let decoded = split_decoded(
        mapper.declarative_decoder_id(),
        U256::from(5_u64),
        oversized,
    );

    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("oversized-ratio split must still emit (observability-only)");
    let action = unwrap_lock_manage(&envelopes[0]);

    assert_eq!(action.kind, LockManageKind::Split);
    assert_eq!(action.from_token_id.to_string(), "5");
    assert_eq!(
        action.split_ratio.as_ref().map(ToString::to_string),
        Some("1500000000000000000".to_owned())
    );
}
