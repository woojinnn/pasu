//! Round 8 P0-1 regression — every newly-added action variant
//! (Borrow / Repay / Liquidate / Stake / ClaimUnstake / Vote /
//! ClaimRewards) must lower into a `PolicyRequest` and reach Cedar
//! evaluation. Before this fix the dispatcher matched none of these
//! variants, so `try_policy_request_from_envelope` returned `Ok(None)`,
//! and the wasm evaluation path silently aggregated an empty verdict list
//! to `Pass` (fail-open).
//!
//! Each test:
//! 1. Builds a minimal envelope for the variant.
//! 2. Lowers it through `policy_request_from_envelope` (the same path the
//!    wasm export uses).
//! 3. Asserts the lowering produced a Cedar request whose `action` matches
//!    the expected kind.
//! 4. Installs a matching `forbid` policy and asserts the verdict is
//!    `Fail`, proving the request actually flows into the engine.

use policy_engine::{
    policy_request_from_envelope, ActionAddress, ActionEnvelope, DecimalString,
    PolicyEngineBuilder, PolicyRequest, Verdict,
};
use serde_json::{json, Value};
use std::str::FromStr as _;

const BLOCK_TIMESTAMP: u64 = 1_700_000_000;
const FROM_HEX: &str = "0x1111111111111111111111111111111111111111";
const TO_HEX: &str = "0x2222222222222222222222222222222222222222";

fn evaluate_with_forbid(action_kind: &str, request: &PolicyRequest) -> Verdict {
    let policy_text = format!(
        "@id(\"test/forbid-{kind}\")\n\
         @severity(\"deny\")\n\
         forbid (principal, action == Action::\"{kind}\", resource);\n",
        kind = action_kind
    );
    let engine = PolicyEngineBuilder::new()
        .add_text(policy_text)
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
        .expect("policy evaluates")
}

fn lower(envelope_value: Value, expected_kind: &str) -> PolicyRequest {
    let envelope: ActionEnvelope =
        serde_json::from_value(envelope_value).expect("envelope deserializes");
    let from = ActionAddress::from_str(FROM_HEX).unwrap();
    let to = ActionAddress::from_str(TO_HEX).unwrap();
    let value_wei = DecimalString::from_str("0").unwrap();
    let request =
        policy_request_from_envelope(&envelope, &from, &to, &value_wei, 1, BLOCK_TIMESTAMP)
            .unwrap_or_else(|| panic!("envelope should lower for action {expected_kind}"));
    assert!(
        request.action.contains(expected_kind),
        "expected request.action to contain {expected_kind}, got {:?}",
        request.action
    );
    request
}

fn assert_forbid_denies(action_kind: &str, request: &PolicyRequest) {
    let verdict = evaluate_with_forbid(action_kind, request);
    match verdict {
        Verdict::Fail(matched) => {
            let policy_ids: Vec<_> = matched.iter().map(|m| m.policy_id.as_str()).collect();
            assert!(
                policy_ids.iter().any(|id| id.contains("forbid")),
                "expected a forbid policy to fire, got {policy_ids:?}"
            );
        }
        other => panic!("expected Verdict::Fail for {action_kind}, got {other:?}"),
    }
}

fn address(value: u8) -> String {
    format!("0x{value:040x}")
}

fn erc20(symbol: &str) -> Value {
    json!({
        "kind": "erc20",
        "address": address(0x10),
        "symbol": symbol,
        "decimals": 18
    })
}

fn amount(kind: &str, value: &str) -> Value {
    json!({ "kind": kind, "value": value })
}

fn native(symbol: &str) -> Value {
    json!({
        "kind": "native",
        "symbol": symbol,
        "decimals": 18
    })
}

#[test]
fn borrow_lowers_and_forbid_denies() {
    let envelope = json!({
        "category": "lending",
        "action": "borrow",
        "fields": {
            "asset": erc20("USDC"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30),
            "onBehalf": address(0x31)
        }
    });
    let request = lower(envelope, "borrow");
    assert_forbid_denies("borrow", &request);
}

#[test]
fn repay_lowers_and_forbid_denies() {
    let envelope = json!({
        "category": "lending",
        "action": "repay",
        "fields": {
            "asset": erc20("USDC"),
            "amount": amount("exact", "1000"),
            "onBehalf": address(0x31),
            "repayKind": "debt_asset"
        }
    });
    let request = lower(envelope, "repay");
    assert_forbid_denies("repay", &request);
}

#[test]
fn liquidate_lowers_and_forbid_denies() {
    let envelope = json!({
        "category": "lending",
        "action": "liquidate",
        "fields": {
            "borrower": address(0x40),
            "debtAsset": erc20("USDC"),
            "liquidationKind": "pool_share"
        }
    });
    let request = lower(envelope, "liquidate");
    assert_forbid_denies("liquidate", &request);
}

#[test]
fn stake_lowers_and_forbid_denies() {
    let envelope = json!({
        "category": "liquid_staking",
        "action": "stake",
        "fields": {
            "tokenIn": native("ETH"),
            "receiptToken": erc20("stETH"),
            "amountIn": amount("exact", "1000"),
            "recipient": address(0x30)
        }
    });
    let request = lower(envelope, "stake");
    assert_forbid_denies("stake", &request);
}

#[test]
fn claim_unstake_lowers_and_forbid_denies() {
    let envelope = json!({
        "category": "liquid_staking",
        "action": "claim_unstake",
        "fields": {
            "tokenOut": native("ETH"),
            "ticket": {},
            "recipient": address(0x30)
        }
    });
    let request = lower(envelope, "claim_unstake");
    assert_forbid_denies("claim_unstake", &request);
}

#[test]
fn vote_lowers_and_forbid_denies() {
    // P0-1 anchor: Curve veCRV voteForGaugeWeights tx routes to
    // `Action::Vote` — without the misc/vote.rs lowering, this envelope
    // silently passed every forbid policy.
    let envelope = json!({
        "category": "misc",
        "action": "vote",
        "fields": {
            "governance": address(0x90),
            "proposalId": "1",
            "support": "for"
        }
    });
    let request = lower(envelope, "vote");
    assert_forbid_denies("vote", &request);
}

#[test]
fn claim_rewards_lowers_and_forbid_denies() {
    let envelope = json!({
        "category": "misc",
        "action": "claim_rewards",
        "fields": {
            "from": address(0x60),
            "recipient": address(0x61)
        }
    });
    let request = lower(envelope, "claim_rewards");
    assert_forbid_denies("claim_rewards", &request);
}

#[test]
fn supply_lowers_and_forbid_denies() {
    // Phase B / F1 — `Action::Supply` joined the dispatch arm so the
    // 6 `crvusd/{wsteth,sfrxeth,wbtc}/addCollateral{,-for}@1.0.0`
    // declarative bundles lower into a Cedar request. Before this fix the
    // dispatcher silently aggregated the empty verdict list to `Pass` —
    // the same fail-open class as Phase 7B `approve` /
    // `set_approval_for_all` (which now lower) and the rest of this
    // module's regressions. This assertion proves the envelope reaches
    // the engine AND a matching forbid policy fires.
    let envelope = json!({
        "category": "lending",
        "action": "supply",
        "fields": {
            "asset": erc20("USDC"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30)
        }
    });
    let request = lower(envelope, "supply");
    assert_forbid_denies("supply", &request);
}

#[test]
fn unlowered_action_still_returns_none() {
    // Defense-in-depth: variants we did NOT add to dispatch (Withdraw,
    // FlashLoan, Delegate, SignMessage, Restake, Donate, …) must still
    // return `Ok(None)` so the wasm export can synthesize a
    // `__engine::action_not_lowered` Warn verdict for them rather than
    // silently passing. Picks `Withdraw` as the canonical representative
    // of the unlowered set — it's structurally adjacent to `Supply`
    // (same `lending` category, mirror operation) so this test exercises
    // the still-missing arm right next to the just-added `Supply` arm.
    // (Phase 7B added `Approve` / `SetApprovalForAll` lowering and
    // Phase B / F1 added `Supply`, so those are no longer in the
    // unlowered set.)
    let envelope = json!({
        "category": "lending",
        "action": "withdraw",
        "fields": {
            "asset": erc20("USDC"),
            "amount": amount("exact", "1000"),
            "recipient": address(0x30)
        }
    });
    let envelope: ActionEnvelope = serde_json::from_value(envelope).expect("envelope deserializes");
    let from = ActionAddress::from_str(FROM_HEX).unwrap();
    let to = ActionAddress::from_str(TO_HEX).unwrap();
    let value_wei = DecimalString::from_str("0").unwrap();
    let request =
        policy_request_from_envelope(&envelope, &from, &to, &value_wei, 1, BLOCK_TIMESTAMP);
    assert!(
        request.is_none(),
        "withdraw has no lowering yet — must return None so exports.rs can emit Warn"
    );
}

// ── Phase 7B — approve / set_approval_for_all dispatch + forbid ───────────
//
// `protocols/erc20/approve.rs` and `set_approval_for_all.rs` static mappers
// emit `Action::Approve` / `Action::SetApprovalForAll`, and the Phase 7B
// `single_emit` arms emit them on the declarative path. Before the matching
// `dispatch.rs` arms landed, both fell through to `Ok(None)` — an empty
// verdict list aggregating to `Pass`. These regressions prove the envelopes
// now lower into a Cedar request AND that a `forbid` policy fires.

#[test]
fn approve_lowers_and_forbid_denies() {
    let envelope = json!({
        "category": "misc",
        "action": "approve",
        "fields": {
            "token": erc20("USDC"),
            "spender": address(0x40),
            "amount": amount("max", "1461501637330902918203684832716283019655932542975"),
            "approvalKind": "permit2",
            "validity": {
                "expiresAt": "1700000900",
                "source": "grant-expiration"
            }
        }
    });
    let request = lower(envelope, "approve");
    assert_forbid_denies("approve", &request);
}

#[test]
fn set_approval_for_all_lowers_and_forbid_denies() {
    // `collection.tokenId` is the collection-wide placeholder `"0"` — the
    // `AssetRef` deserializer requires a `tokenId` for `erc721` assets even
    // though `setApprovalForAll` is not scoped to a single token id.
    let envelope = json!({
        "category": "misc",
        "action": "set_approval_for_all",
        "fields": {
            "collection": {
                "kind": "erc721",
                "address": address(0x11),
                "tokenId": "0",
                "symbol": "UNI-V3-POS"
            },
            "operator": address(0x41),
            "approved": true
        }
    });
    let request = lower(envelope, "set_approval_for_all");
    assert_forbid_denies("set_approval_for_all", &request);
}
