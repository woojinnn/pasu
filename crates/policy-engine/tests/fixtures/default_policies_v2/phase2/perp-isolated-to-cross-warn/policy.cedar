// scopeball default v2 policy — explorer id: PERP-06 (switching margin mode from
// isolated to cross).
//
// Cross margin pools all positions against one collateral balance, so one bad
// trade can drag down otherwise-healthy positions. Detecting the RISKY DIRECTION
// (isolated → cross) needs the CURRENT mode, which lives in wallet state
// (`PerpPosition.margin_mode`, State₁) — the action carries only `newMode`
// (`context.newMode`, a comparable String), not the current one. The manifest's
// `perp.margin_mode_transition` reads the durable mode for the market and compares
// it to `newMode`, emitting the `Bool` `isolatedToCross`. Defined-not-wired →
// `optional` (fail-open).
@id("perp-isolated-to-cross-warn")
@severity("warn")
@reason("Switching this market to cross margin lets its losses pull down your other positions")
forbid(principal, action == Perp::Action::"ChangeMarginMode", resource)
when {
  context has custom
  && context.custom has isolatedToCross
  && context.custom.isolatedToCross
};
