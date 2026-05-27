//! Aerodrome Gauge edge case tests (T-TEST-AERO-GAUGE).
//!
//! Exercises the Phase 8 Round 3 `build_lp_stake_envelope` /
//! `build_lp_unstake_envelope` (commit `454cff5`), the Round 5a
//! `build_claim_rewards_envelope` (commit `502dabe`), and the Round 4
//! Gauge 3 bundles (commit `61b9fae`) through the `DeclarativeMapper`
//! pipeline end-to-end.
//!
//! Bundles under test (registry/manifests/aerodrome/gauge):
//!
//!   * `deposit@1.0.0`   — selector `0xb6b55f26` — single-arg
//!     `deposit(uint256)`. Emits `LpStake` with `gauge = $.tx.to`,
//!     `lpToken.address = $.tx.to` (Aerodrome's gauge address doubles as
//!     the LP receipt token reference), `amount.kind = exact`, and
//!     `recipient = $.tx.from` (builder default — no explicit recipient
//!     arg on this overload).
//!   * `withdraw@1.0.0`  — selector `0x2e1a7d4d` — `withdraw(uint256)`.
//!     Mirror of deposit; emits `LpUnstake`.
//!   * `getReward@1.0.0` — selector `0xc00007b0` — V2-gauge
//!     `getReward(address)`. Emits `ClaimRewards` where `from` /
//!     `recipient` both come from `$.args.account` (NOT `tx.from` —
//!     callers can claim on behalf of any account) and `rewardTokens[0]`
//!     is the hard-coded AERO token address `0x940181a9...8631`.
//!
//! Coverage focus (Phase 8 Round 6 — T-TEST-AERO-GAUGE):
//!
//!   1. `deposit(1000e18)` — happy path. envelope = `LpStake { gauge =
//!      tx.to, lpToken.kind = erc20, lpToken.address = tx.to,
//!      amount.value = 1000e18, recipient = tx.from }`.
//!   2. `deposit(0)` — zero amount. Builder accepts (declarative path
//!      is observability-only; `forbid-zero-amount-lp-stake` belongs to
//!      the Cedar engine downstream).
//!   3. `deposit(uint256)` overload-recipient — confirms the single-arg
//!      bundle binds `recipient` from `$.tx.from`, not from any
//!      `$.args.to`. A hypothetical 2-arg `deposit(uint256, address)`
//!      overload would publish under a different selector and a separate
//!      bundle; this test pins the single-arg semantics.
//!   4. `withdraw(500e18)` — happy path. envelope = `LpUnstake { gauge
//!      = tx.to, amount.value = 500e18 }`.
//!   5. `withdraw(u256::MAX)` — extreme amount. The builder does not
//!      cap or validate `amount`; runtime revert (e.g. "Gauge: amount >
//!      balance") is a chain-side concern, not a pre-sign mapper one.
//!   6. `getReward($.args.account)` — V2-gauge claim. `account` differs
//!      from `tx.from` to prove `recipient` follows the arg (caller
//!      claiming on behalf of another LP).
//!
//! Tests are read-only on production code — `src/` is untouched. The
//! bundle JSONs are loaded directly via `include_str!` from the
//! registry tree.
//!
//! ## Source mapping
//!
//! * Aerodrome V2 Gauge `deposit(uint256)` selector `0xb6b55f26` —
//!   https://github.com/aerodrome-finance/contracts (Gauge.sol)
//! * V2 Gauge `withdraw(uint256)` selector `0x2e1a7d4d`
//! * V2 Gauge `getReward(address)` selector `0xc00007b0`
//! * CL gauges (Slipstream) use `getReward(uint256)` — out of PoC scope
//!   per Phase 8 plan §13.1 Round 4.5; this file covers V2 gauges only.

use std::str::FromStr as _;

use abi_resolver::{DecodedArg, DecodedCall, DecodedValue, DecoderId};
use alloy_primitives::U256;
use mappers::declarative::{types::AdapterFunctionBundle, DeclarativeMapper};
use mappers::mapper::{MapContext, Mapper};
use mappers::EmptyTokenRegistry;
use policy_engine::action::{
    Action, ActionEnvelope, Address, AmountKind, AssetKind, DecimalString,
};

// ───────────────────────────────────────────────────────────────────────────
// Bundle fixtures — loaded straight from the registry tree.
// ───────────────────────────────────────────────────────────────────────────

const GAUGE_DEPOSIT_BUNDLE: &str =
    include_str!("../../../../registry/manifests/aerodrome/gauge/deposit@1.0.0.json");
const GAUGE_WITHDRAW_BUNDLE: &str =
    include_str!("../../../../registry/manifests/aerodrome/gauge/withdraw@1.0.0.json");
const GAUGE_GET_REWARD_BUNDLE: &str =
    include_str!("../../../../registry/manifests/aerodrome/gauge/getReward@1.0.0.json");

// ───────────────────────────────────────────────────────────────────────────
// Address fixtures.
// ───────────────────────────────────────────────────────────────────────────

/// First V2 gauge from `deposit@1.0.0.json` `match.to[]`. Any address in
/// the list works — the bundle does not branch on which one. Lowercased
/// (the `Address` newtype normalizes on `from_str`).
fn gauge_addr() -> Address {
    Address::from_str("0x4f09bab2f0e15e2a078a227fe1537665f55b8360").unwrap()
}

/// EOA caller — `$.tx.from`.
fn caller() -> Address {
    Address::from_str("0x00000000000000000000000000000000000000aa").unwrap()
}

/// A different LP account — used to prove `getReward.account` is NOT
/// `tx.from`.
fn other_lp() -> Address {
    Address::from_str("0x1234567890abcdef1234567890abcdef12345678").unwrap()
}

/// AERO token address hard-coded in the `getReward@1.0.0` bundle's
/// `rewardTokens[0].address` literal.
fn aero_token() -> Address {
    Address::from_str("0x940181a94a35a4569e4529a3cdfb74e38fd98631").unwrap()
}

// ───────────────────────────────────────────────────────────────────────────
// Context helper.
// ───────────────────────────────────────────────────────────────────────────

struct Ctx {
    registry: EmptyTokenRegistry,
    from: Address,
    to: Address,
    value: DecimalString,
}

impl Ctx {
    /// Default `ctx.to = gauge_addr()`. Chain id = 8453 (Base) to match
    /// the bundles' `match.chain_ids = [8453]`.
    fn new() -> Self {
        Self {
            registry: EmptyTokenRegistry,
            from: caller(),
            to: gauge_addr(),
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
// DecodedCall builders.
// ───────────────────────────────────────────────────────────────────────────

/// Build a `DecodedCall` for V2 Gauge `deposit(uint256)`.
fn gauge_deposit_decoded(decoder_id: DecoderId, amount: U256) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature: "deposit(uint256)".into(),
        args: vec![DecodedArg {
            name: "_amount".into(),
            abi_type: "uint256".into(),
            value: DecodedValue::Uint(amount),
        }],
        nested: vec![],
    }
}

/// Build a `DecodedCall` for V2 Gauge `withdraw(uint256)`.
fn gauge_withdraw_decoded(decoder_id: DecoderId, amount: U256) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature: "withdraw(uint256)".into(),
        args: vec![DecodedArg {
            name: "_amount".into(),
            abi_type: "uint256".into(),
            value: DecodedValue::Uint(amount),
        }],
        nested: vec![],
    }
}

/// Build a `DecodedCall` for V2 Gauge `getReward(address)`.
fn gauge_get_reward_decoded(decoder_id: DecoderId, account: Address) -> DecodedCall {
    DecodedCall {
        decoder_id,
        function_signature: "getReward(address)".into(),
        args: vec![DecodedArg {
            name: "_account".into(),
            abi_type: "address".into(),
            value: DecodedValue::Address(account),
        }],
        nested: vec![],
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Mapper loaders + envelope unwrappers.
// ───────────────────────────────────────────────────────────────────────────

fn load_deposit_mapper() -> DeclarativeMapper {
    let bundle: AdapterFunctionBundle =
        serde_json::from_str(GAUGE_DEPOSIT_BUNDLE).expect("gauge deposit bundle parses");
    DeclarativeMapper::new(bundle)
}

fn load_withdraw_mapper() -> DeclarativeMapper {
    let bundle: AdapterFunctionBundle =
        serde_json::from_str(GAUGE_WITHDRAW_BUNDLE).expect("gauge withdraw bundle parses");
    DeclarativeMapper::new(bundle)
}

fn load_get_reward_mapper() -> DeclarativeMapper {
    let bundle: AdapterFunctionBundle =
        serde_json::from_str(GAUGE_GET_REWARD_BUNDLE).expect("gauge getReward bundle parses");
    DeclarativeMapper::new(bundle)
}

fn unwrap_lp_stake(envelope: &ActionEnvelope) -> &policy_engine::action::misc::LpStakeAction {
    match &envelope.action {
        Action::LpStake(s) => s,
        other => panic!("expected LpStakeAction, got {other:?}"),
    }
}

fn unwrap_lp_unstake(envelope: &ActionEnvelope) -> &policy_engine::action::misc::LpUnstakeAction {
    match &envelope.action {
        Action::LpUnstake(u) => u,
        other => panic!("expected LpUnstakeAction, got {other:?}"),
    }
}

fn unwrap_claim_rewards(
    envelope: &ActionEnvelope,
) -> &policy_engine::action::misc::ClaimRewardsAction {
    match &envelope.action {
        Action::ClaimRewards(c) => c,
        other => panic!("expected ClaimRewardsAction, got {other:?}"),
    }
}

fn amount_value_string(value: &Option<DecimalString>) -> String {
    value
        .as_ref()
        .map(ToString::to_string)
        .expect("amount.value should be set on exact-kind amounts")
}

// ───────────────────────────────────────────────────────────────────────────
// Tests
// ───────────────────────────────────────────────────────────────────────────

/// **T1: deposit happy path — `amount = 1000e18` (`1_000 * 1e18`)**.
///
/// The bundle binds `gauge = $.tx.to`, `lpToken.address = $.tx.to`, and
/// `recipient = $.tx.from`. All three values come from `MapContext`,
/// not from calldata, so the single `amount` arg is the only calldata
/// dependency.
#[test]
fn gauge_deposit_emits_lp_stake_envelope() {
    let mapper = load_deposit_mapper();
    let ctx = Ctx::new();
    // 1_000 * 10^18 — picked > u64::MAX boundary risk to also exercise
    // U256-to-decimal-string conversion (24-digit base-10).
    let amount = U256::from(1_000_000_000_000_000_000_000_u128);

    let decoded = gauge_deposit_decoded(mapper.declarative_decoder_id(), amount);
    let envelopes = mapper.map(&ctx.map_ctx(), &decoded).expect("deposit maps");
    assert_eq!(envelopes.len(), 1, "single_emit produces one envelope");

    let stake = unwrap_lp_stake(&envelopes[0]);
    assert_eq!(stake.gauge, gauge_addr(), "gauge = $.tx.to");
    // Phase D B-3 fix: lpToken.kind = "unknown" (was emitting `erc20` with
    // `lpToken.address = $.tx.to` placeholder, which falsely implied the
    // gauge address was the LP token. The actual LP token is un-derivable
    // from `deposit(uint256)` calldata.)
    assert_eq!(stake.lp_token.asset.kind, AssetKind::Unknown);
    assert_eq!(
        stake.lp_token.asset.address, None,
        "lpToken.address = None (kind:unknown carries no address)"
    );
    assert_eq!(stake.lp_token.amount.kind, AmountKind::Exact);
    assert_eq!(
        amount_value_string(&stake.lp_token.amount.value),
        "1000000000000000000000",
        "amount.value preserves the 21-digit decimal string"
    );
    assert_eq!(stake.recipient, caller(), "recipient = $.tx.from");
}

/// **T2: deposit zero amount — envelope emitted regardless**.
///
/// The declarative path is observability-only. A `forbid-zero-amount`
/// Cedar default policy can reject the envelope downstream, but the
/// interpreter does not gate emission on `amount > 0` — that would
/// make declarative behaviour diverge from the static mapper baseline.
#[test]
fn gauge_deposit_zero_amount_emits_envelope_unchanged() {
    let mapper = load_deposit_mapper();
    let ctx = Ctx::new();

    let decoded = gauge_deposit_decoded(mapper.declarative_decoder_id(), U256::ZERO);
    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("deposit(0) still maps (no builder-side rejection)");
    let stake = unwrap_lp_stake(&envelopes[0]);

    assert_eq!(stake.lp_token.amount.kind, AmountKind::Exact);
    assert_eq!(
        amount_value_string(&stake.lp_token.amount.value),
        "0",
        "amount.value = \"0\" — Cedar handles forbid-zero policy"
    );
    // Other fields must still resolve normally.
    assert_eq!(stake.gauge, gauge_addr());
    assert_eq!(stake.recipient, caller());
}

/// **T3: single-arg `deposit(uint256)` recipient = `$.tx.from`**.
///
/// Aerodrome's `Gauge.sol` exposes only one `deposit` selector — the
/// single-arg form covered here. A hypothetical
/// `deposit(uint256, address)` overload would have a *different*
/// selector (`0x6e553f65` per the ERC-4626-style convention) and live in
/// a separate bundle. This test pins the single-arg semantics:
/// regardless of who the caller wants to credit, the bundle binds
/// `recipient` to `$.tx.from` since calldata carries no explicit
/// recipient.
#[test]
fn gauge_deposit_single_arg_recipient_is_tx_from() {
    let mapper = load_deposit_mapper();
    // Use a `from` address that's clearly different from `to` (gauge)
    // and from the AERO / fixture addresses, to surface any accidental
    // wiring mix-up.
    let from = Address::from_str("0xfeedfacefeedfacefeedfacefeedfacefeedface").unwrap();
    let to = gauge_addr();
    let value = DecimalString::from_str("0").unwrap();
    let registry = EmptyTokenRegistry;
    let ctx = MapContext::new(8453, &from, &to, &value, Some(1_700_000_000), &registry);

    let decoded = gauge_deposit_decoded(mapper.declarative_decoder_id(), U256::from(42_u64));

    let envelopes = mapper.map(&ctx, &decoded).expect("deposit maps");
    let stake = unwrap_lp_stake(&envelopes[0]);

    assert_eq!(
        stake.recipient, from,
        "single-arg deposit binds recipient to $.tx.from, NOT to any \
         non-existent $.args.to"
    );
    // gauge is still tied to ctx.to.
    assert_eq!(stake.gauge, to);
    // Phase D B-3 fix: lpToken.kind = "unknown" → address = None.
    assert_eq!(stake.lp_token.asset.address, None);
}

/// **T4: withdraw happy path — `amount = 500e18`**.
///
/// Mirror of T1 for `LpUnstake`. The bundle layout is byte-for-byte
/// identical except for `action: lp_unstake` and selector `0x2e1a7d4d`.
#[test]
fn gauge_withdraw_emits_lp_unstake_envelope() {
    let mapper = load_withdraw_mapper();
    let ctx = Ctx::new();
    // 500 * 1e18.
    let amount = U256::from(500_000_000_000_000_000_000_u128);

    let decoded = gauge_withdraw_decoded(mapper.declarative_decoder_id(), amount);
    let envelopes = mapper.map(&ctx.map_ctx(), &decoded).expect("withdraw maps");
    assert_eq!(envelopes.len(), 1);

    let unstake = unwrap_lp_unstake(&envelopes[0]);
    assert_eq!(unstake.gauge, gauge_addr());
    // Phase D B-3 fix: withdraw bundle also emits lpToken.kind = "unknown"
    // because the LP token is un-derivable from `withdraw(uint256)` calldata.
    assert_eq!(unstake.lp_token.asset.kind, AssetKind::Unknown);
    assert_eq!(unstake.lp_token.asset.address, None);
    assert_eq!(unstake.lp_token.amount.kind, AmountKind::Exact);
    assert_eq!(
        amount_value_string(&unstake.lp_token.amount.value),
        "500000000000000000000",
        "withdraw amount preserves 21-digit decimal"
    );
    assert_eq!(unstake.recipient, caller());
}

/// **T5: withdraw `amount = u256::MAX` — emitted verbatim**.
///
/// On-chain this would revert (`Gauge: insufficient balance`) but
/// pre-sign analysis is decoupled from runtime semantics. The
/// `DecimalString` round-trip for `2^256 - 1` is also covered by
/// `edge_v3.rs::v3_exact_input_max_uint256_amount_in_succeeds`; this
/// assertion pins the same property for the gauge bundle.
#[test]
fn gauge_withdraw_max_uint256_amount_emits_envelope() {
    let mapper = load_withdraw_mapper();
    let ctx = Ctx::new();

    let decoded = gauge_withdraw_decoded(mapper.declarative_decoder_id(), U256::MAX);
    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("max u256 amount does not fault the builder");
    let unstake = unwrap_lp_unstake(&envelopes[0]);

    let expected_max =
        "115792089237316195423570985008687907853269984665640564039457584007913129639935";
    assert_eq!(unstake.lp_token.amount.kind, AmountKind::Exact);
    assert_eq!(
        amount_value_string(&unstake.lp_token.amount.value),
        expected_max,
        "u256::MAX serializes as the canonical 78-digit decimal string"
    );
}

/// **T6: V2 gauge `getReward(account)` — recipient follows arg, not
/// `tx.from`**.
///
/// The bundle binds both `from` and `recipient` to `$.args.account`.
/// Sets `ctx.from = caller()` (some EOA) and `account = other_lp()` to
/// prove the envelope captures the *target* LP, not the caller. Also
/// asserts the hard-coded AERO `rewardTokens[0]` literal survives the
/// `set_nested` round-trip the builder uses for array-indexed paths.
#[test]
fn gauge_get_reward_emits_claim_rewards_with_account_recipient() {
    let mapper = load_get_reward_mapper();
    let ctx = Ctx::new();
    let account = other_lp();

    let decoded = gauge_get_reward_decoded(mapper.declarative_decoder_id(), account.clone());
    let envelopes = mapper
        .map(&ctx.map_ctx(), &decoded)
        .expect("getReward maps");
    assert_eq!(envelopes.len(), 1);

    let claim = unwrap_claim_rewards(&envelopes[0]);

    // `from` and `recipient` both follow `$.args.account` — proves the
    // bundle does NOT silently substitute `tx.from`.
    assert_eq!(
        claim.from, account,
        "claim.from = $.args.account, distinct from tx.from"
    );
    assert_eq!(claim.recipient, account, "claim.recipient = $.args.account");
    assert_ne!(
        claim.from,
        caller(),
        "explicit sanity check — account differs from tx.from in this test"
    );

    // `source` carries the gauge address ($.tx.to) and the literal
    // "Aerodrome Gauge" label.
    let source = claim.source.as_ref().expect("source present");
    assert_eq!(source.address.as_ref(), Some(&gauge_addr()));
    assert_eq!(source.label.as_deref(), Some("Aerodrome V2 Gauge"));

    // `rewardTokens[0]` hard-coded to AERO.
    let rewards = claim
        .reward_tokens
        .as_ref()
        .expect("rewardTokens emitted (bundle has a literal entry)");
    assert_eq!(rewards.len(), 1);
    assert_eq!(rewards[0].kind, AssetKind::Erc20);
    assert_eq!(rewards[0].address.as_ref(), Some(&aero_token()));

    // NFT-related fields stay None for V2 gauges (Slipstream CL gauges
    // would set `nft` / `tokenId`, but they use the `getReward(uint256)`
    // selector and are out of PoC scope per plan §13.1).
    assert!(claim.nft.is_none(), "V2 gauge has no NFT position");
    assert!(claim.token_id.is_none());
}
