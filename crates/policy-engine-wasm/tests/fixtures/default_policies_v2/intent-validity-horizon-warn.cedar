// scopeball default v2 policy — explorer id: UNI-11 (UniswapX/CoW order TTL too
// long → free option for the filler).
//
// A signed open order is effectively a free option for the filler/solver: a long
// TTL lets the market move against you before it fills. `validUntil` is a base
// `Long` (unix seconds) on `Amm::SignIntentOrderContext`, but the HORIZON
// (validUntil − now) needs the evaluation clock, which Cedar lacks. The method
// `intent.validity_horizon_sec` subtracts now and emits the enriched `Long`
// `validityHorizonSec`. `custom` and the call are optional, so the path is
// `has`-guarded. 3600 s (1h) horizon ceiling here.
@id("intent-validity-horizon-warn")
@severity("warn")
@reason("This signed order stays valid for a long time — the filler can wait for the market to move against you")
forbid(principal, action == Amm::Action::"SignIntentOrder", resource)
when {
  context has custom
  && context.custom has validityHorizonSec
  && context.custom.validityHorizonSec > 3600
};
