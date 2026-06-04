// scopeball default v2 policy — explorer id: PERP-04 (funding is heavily against
// your entry direction).
//
// A position can be directionally right and still bleed: an adverse funding rate
// quietly drains the position the whole time it's held — easy to miss at entry.
// Whether funding HELPS or HURTS depends on `side` (a long pays positive funding,
// a short receives it), so the raw `fundingRate` (a `Decimal`-as-String) must be
// SIGN-ALIGNED to the order side before its magnitude means anything. Cedar can
// neither sign-flip nor compare the String, so the manifest's
// `perp.funding_adverse_rate` folds `side` against `fundingRate` and emits the
// adverse magnitude as the `decimal` `fundingAdverseRate`. Defined-not-wired →
// `optional` (fail-open). Warn over 0.05% per interval (user config).
@id("perp-funding-adverse-warn")
@severity("warn")
@reason("Funding is strongly against your entry direction — it will erode this position over time")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has fundingAdverseRate
  && context.custom.fundingAdverseRate.greaterThan(decimal("0.0005"))
};
