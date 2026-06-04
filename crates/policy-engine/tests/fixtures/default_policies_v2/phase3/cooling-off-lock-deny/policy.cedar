// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// BLOCKED-BY-STATE: needs a NEW per-owner user-activated self-lock surface
// (per-owner: { lockActive, lockUntilUnix, activatedAtUnix }). The lock is SET
// only by an explicit user action and may be CLEARED only when now >= lockUntilUnix
// — there is no early-unset path (the "조기해제 불가" precommitment is enforced in
// the state layer, not in this policy). That self-lock state does not exist on the
// live path today (simulation-state's PendingTx/PendingStatus FSM is unrelated to a
// user precommitment timer and is unwired to the live signing path), so "am I
// currently locked?" cannot be derived. Verify will demote this to needs-new-state;
// the files below are the TARGET shape for when that lock surface lands.
//
// scopeball default v2 policy — user-activated cooling-off lock.
// Personal-trading (TradFi pre-trade) self-control / precommitment family.
//
// Rule: deny this signing action while the wallet's user-activated cooling-off
// lock is in effect. The user binds their future self; once activated the lock
// cannot be cancelled early — only the passage of time (now >= lockUntilUnix)
// releases it. Exchange precedent: Binance/Bybit "cooling-off period"; regulatory
// precedent: UK FCA cooling-off mandate (2023-10-08).
//
// WHY new state is required:
//   "is my user-activated lock currently in effect" is a per-owner precommitment
//   record (lockActive AND now < lockUntilUnix) that persists ACROSS requests and
//   is set by a prior explicit user action — it is not a field of THIS action's
//   context (live_inputs describe only the transfer being signed). The manifest
//   declares `selfcontrol.cooling_off_active`, projecting the verdict into the
//   `Bool` `coolingOffActive`. Defined-not-wired → `optional` (fail-open), so the
//   field is `has`-guarded; no lock / expired lock ⇒ false ⇒ never fires.
//
// ⚠️ ASYMMETRY WARNING (2026-06-03, user critique verified): targeting
// `Token::Erc20Transfer` is the WRONG canonical variant for a SELF-control lock.
// A bare transfer is most often risk-REDUCING — a withdrawal to a CEX to SELL, or
// a move to cold storage — and a non-custodial deny-lock that blocks it TRAPS the
// user's funds (the inverse of self-protection; the gambling analogue self-excludes
// from BETTING, never from WITHDRAWING your balance). A cooling-off / self-exclusion
// lock must block only RISK-INCREASING entries (OpenPosition, IncreasePosition,
// AddMargin, leverage-up, stable->risk buy-swap, entry-purpose Erc20Approve) and
// must NEVER block the risk-REDUCING EXEMPT set (ClosePosition, ReducePosition,
// SetStopLoss/TakeProfit, risk->stable sell-swap, Repay/Deleverage/RemoveMargin,
// RevokeApproval — and a withdrawal/sell Erc20Transfer). So this draft's trigger
// should move to the risk-entry set; a future policy-factory build lint must assert
// every self-control deny's forbid set ∩ EXEMPT set is EMPTY.
// Detail: reports/self-control-realized-asymmetry-refinement.md, self-control-state-design.md §2.5.
//
// SCOPE NOTE: Cedar forbids per action-variant with no wildcard. Full coverage of a
// self-control lock = a fan-out over the RISK-ENTRY BLOCK set ONLY (never the EXEMPT
// set above) or a new tx-level deny primitive that gates risk-entries — both depend
// on the same lock state. (This file's current Erc20Transfer target is retained only
// as the pre-critique shape and is superseded by the asymmetry warning above.)
@id("cooling-off-lock-deny")
@severity("deny")
@reason("Your self-set cooling-off lock is active — signing is blocked until it expires and cannot be cancelled early")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has coolingOffActive
  && context.custom.coolingOffActive
};
