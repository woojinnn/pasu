// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// BLOCKED-BY-STATE: needs a per-owner closed-position loss/liquidation EVENT
// history surface that records, for the most recent losing/liquidated close,
// { closedAtUnix, market.symbol, side, realizedPnlSign, wasLiquidated }. That
// wallet state does not exist on the live perp path today — simulation-state has
// a PendingTx/PendingStatus FSM but it is NOT wired to live perp closes, so
// neither the trailing "seconds since last loss" window NOR the last-loss
// market/side can be derived. Verify will demote this to needs-new-state; the
// files below are the TARGET shape for when that history surface lands.
//
// DUP NOTE: this is the same threat + same state surface as the sibling draft
// `perp-revenge-trade-cooldown-warn`. The ONLY difference is precision: that one
// warns on ANY new open within the cooldown; this one warns ONLY when you
// re-enter the SAME market on the SAME side that just liquidated/lost — the
// defining revenge-trade signature (e.g. re-longing the exact asset that wiped
// you), which strips the false positives from rotating into a different market
// or hedging the other way. Recorded as dupOf perp-revenge-trade-cooldown-warn.
//
// scopeball default v2 policy — revenge re-entry cooldown (same asset, same side).
// Personal-trading (TradFi pre-trade) self-control family.
//
// Evidence: the 2026-04-17 cascade liquidated ~$9.26B of longs ($4.69B on
// Binance alone), described as destroying more accounts than any market crash —
// driven by traders re-longing the very asset that just liquidated them
// (benzinga / xt-medium / coinbureau / addictioncenter, 4 distinct sources).
//
// WHY new state is required:
//   The match needs (a) seconds since the last loss/liquidation — a sliding
//   window over PAST closed events — AND (b) the market.symbol + side of that
//   last losing close, to compare against THIS OpenPosition's market/side. Both
//   are cross-request EVENT HISTORY; neither is in the OpenPosition context
//   (live_inputs describe only the new entry; PerpAccountState carries OPEN
//   positions, not a closed-loss-event log). The manifest declares
//   `perp.last_loss_event`, projecting elapsed seconds (Long), and the last-loss
//   market symbol + side (String) into custom context. Defined-not-wired →
//   `optional` (fail-open), so every field is `has`-guarded. No prior loss ⇒ the
//   method returns a very large `secondsSinceLastLoss` (sentinel) ⇒ never fires.
//   Cooldown 1800s (30 min) is user config.
@id("perp-revenge-reentry-cooldown-warn")
@severity("warn")
@reason("You just took a loss or got liquidated on this market and side — re-entering it this soon is a revenge trade")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has secondsSinceLastLoss
  && context.custom.secondsSinceLastLoss < 1800
  && context.custom has lastLossMarketSymbol
  && context.custom.lastLossMarketSymbol == context.market.symbol
  && context.custom has lastLossSide
  && context.custom.lastLossSide == context.side
};
