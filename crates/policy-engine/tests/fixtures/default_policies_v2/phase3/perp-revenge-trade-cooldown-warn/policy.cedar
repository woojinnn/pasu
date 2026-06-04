// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// BLOCKED-BY-STATE: needs a closed-position loss/liquidation EVENT history surface
// (per-owner: last-closed timestamp + realized-PnL sign + liquidation flag). That
// wallet state does not exist on the live path today — simulation-state has a
// PendingTx/PendingStatus FSM but it is not wired to live perp closes, so the
// trailing "seconds since last loss" window cannot be derived. Verify will demote
// this to needs-new-state; the files below are the TARGET shape for when that
// history surface lands.
//
// scopeball default v2 policy — revenge-trade cooldown after a loss/liquidation.
// Personal-trading (TradFi pre-trade) self-control family.
//
// Rule: warn when the wallet OPENS a new perp position within the cooldown window
// after its most recent realized-loss / liquidation event. Re-entering hot off a
// loss and compounding it is the textbook revenge-trade blow-up.
//
// WHY new state is required:
//   "time since my last losing/liquidated close" is a sliding window over PAST
//   closed events — cross-request EVENT HISTORY, not a field of this OpenPosition
//   context (live_inputs describe the new entry only, and PerpAccountState carries
//   open positions, not a loss-event log). The manifest declares
//   `perp.seconds_since_last_loss`, projecting the elapsed seconds into the `Long`
//   `secondsSinceLastLoss`. Defined-not-wired → `optional` (fail-open), so the
//   field is `has`-guarded; `Long` compares with `<` directly. No prior loss ⇒ the
//   method returns a very large value ⇒ never fires. Cooldown 1800s (30 min) is
//   user config.
@id("perp-revenge-trade-cooldown-warn")
@severity("warn")
@reason("You just took a loss or got liquidated — opening a new position this soon is a revenge trade")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has secondsSinceLastLoss
  && context.custom.secondsSinceLastLoss < 1800
};
