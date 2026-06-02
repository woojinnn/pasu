// scopeball default v2 policy — explorer id: PERP-12 (stop-loss / take-profit
// trigger placed on the wrong side of price).
//
// A long with its stop-loss ABOVE mark (or a take-profit BELOW it) fires instantly
// and backwards — an unambiguous mistake. Correctness is a relation between
// `triggerPrice`, `side`, `orderKind` (SL vs TP) and the live `markPrice`. Both
// `triggerPrice` and `markPrice` are `Decimal`-as-String — Cedar cannot order
// them — and the SL/TP-vs-side direction rule is a multi-branch comparison. The
// manifest's `perp.stop_trigger_misplaced` evaluates the consistency check and
// emits the `Bool` `stopTriggerMisplaced`. Defined-not-wired → `optional`
// (fail-open).
@id("perp-stop-trigger-misplaced-warn")
@severity("warn")
@reason("This stop/take-profit trigger is on the wrong side of the current price and would fire immediately")
forbid(principal, action == Perp::Action::"PlaceStopOrder", resource)
when {
  context has custom
  && context.custom has stopTriggerMisplaced
  && context.custom.stopTriggerMisplaced
};
