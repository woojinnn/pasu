// scopeball default v2 policy — explorer id: PERP-02 (entry leaves the
// liquidation price too close to mark).
//
// Even with the right direction, a liq_price that sits right against mark gets
// taken out by a single wick (Synthetix shipped "Wick Insurance" for exactly
// this). The post-entry liq_price is a State₂ value the reducer recomputes
// (liq_price = entry ∓ Σcollateral/size_base), then measured as a bp distance to
// live mark. Both `markPrice` and the resulting liq_price are `Decimal`-as-String
// — not Cedar-comparable — so the manifest's `perp.liq_distance_bp` projects the
// already-computed distance as the `Long` `liqDistanceBp`. Defined-not-wired →
// `optional` (fail-open). Warn when the buffer is under 100 bp (1%, user config).
@id("perp-liq-distance-thin-warn")
@severity("warn")
@reason("This entry would sit dangerously close to its liquidation price")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has liqDistanceBp
  && context.custom.liqDistanceBp < 100
};
