// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// scopeball candidate — Degen Self-Control Set / daily-loss-limit-lockout-deny.
//
// Rule: DENY any new perp entry once the wallet's pre-set daily loss limit
// (-$X or -N%) has been hit today. The lockout latches: once tripped it cannot
// be lifted until the next session boundary (Topstep / TopstepX / ProjectX /
// TradeBlocker all ship exactly this precommitment — "70% of trading mistakes
// are impulsive", and the lock is intentionally non-overridable same-day).
//
// BLOCKED-BY-STATE: this needs two state surfaces the stateless policy-engine
// does NOT have today —
//   (1) dailyRealizedPnlUsd : a per-wallet realized-PnL accumulator that resets
//       at a session boundary (cross-request; not a field of THIS request), and
//   (2) lossLimitLockedToday : a LATCHED flag that, once set, the user cannot
//       clear until the next session (no "sticky until reset" concept exists in
//       any current state/aggregate surface).
// Until that state ships, the `state.daily_loss_lockout` method is `optional`
// (fail-open): `context.custom has lossLimitLockedToday` is false and this
// policy no-ops rather than SystemFail. It activates once the state lands.
//
// Target shape (gate will reject as needs-new-state until state exists):
@id("daily-loss-limit-lockout-deny")
@severity("deny")
@reason("You hit your pre-set daily loss limit today — new perp entries are locked until tomorrow")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has lossLimitLockedToday
  && context.custom.lossLimitLockedToday
};
