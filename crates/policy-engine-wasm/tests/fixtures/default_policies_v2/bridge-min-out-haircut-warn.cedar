// scopeball default v2 policy — explorer id: BRG-05 (liquidity-bridge minimum
// received amount / slippage floor is too harsh).
//
// On liquidity bridges (Stargate, Hop) a larger send can lose meaningfully to
// slippage. The minimum-received floor (`minAmountLD` / `amountOutMin`) that
// the UI baked into the calldata should be sanity-checked against a fair quote.
// With no dedicated bridge decode the move lands as `Core::Unknown`: the min-out
// is buried in the `calldata` hex String (not a parsed comparable field) and the
// haircut is `(fair_quote − min_out) / fair_quote` — arithmetic over a hex blob
// plus an oracle quote that Cedar cannot perform. This is a METHOD:
// `bridge.min_out_haircut_bp` extracts the min-out from the calldata, prices the
// send against a fair quote, and returns the haircut in basis points (Long).
//
// `custom` and the field are optional; the call is `optional` and fail-opens
// until the fact host ships. The threshold (500 bp = 5%) is config inlined for
// this slice (design proposal F#2 — personalized supply is a later increment).
// Warn when the haircut exceeds the threshold.
@id("bridge-min-out-haircut-warn")
@severity("warn")
@reason("Bridge minimum-received floor is far below a fair quote — you may receive much less than expected")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has minOutHaircutBp
  && context.custom.minOutHaircutBp > 500
};
