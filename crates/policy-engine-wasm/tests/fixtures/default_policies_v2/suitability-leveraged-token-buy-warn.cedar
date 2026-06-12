// dambi default v2 policy — explorer id: D1 / SUIT (leverage-class token
// purchase). Personal-trading (TradFi suitability) family.
//
// Rule: deny buying a token whose risk class is "Leveraged" (2x/3x products such
// as ETH2x / TQQQ-style on-chain leveraged tokens), and additionally guard the
// portfolio share of the whole leveraged class. The PERP-01 action-leverage cap
// is a SEPARATE control — here the leverage is BAKED INTO the asset, so it never
// appears in the swap's action params (no `leverage` field on Amm::Swap).
//
// WHY a method is required (the point of this fixture):
//   the acquired token (`tokenOut`) is a Core::TokenRef record. Its risk class
//   ("Leveraged" vs plain) is an EXTERNAL classification not carried in calldata
//   or live_inputs, and the class's portfolio share is a whole-wallet fold. The
//   manifest declares `token.risk_class`, projecting `isLeveraged` (Bool) and
//   `levClassPct` (decimal) into context.custom. Defined-not-wired → the call is
//   `optional` (fail-open), so each field is `has`-guarded. Deny the buy when the
//   asset is leveraged-class OR when the leveraged class would exceed the user's
//   10% cap (cap is user config).
@id("suitability-leveraged-token-buy-warn")
@severity("warn")
@reason("This token has leverage baked into the product — leveraged-class assets are blocked or capped by your suitability profile")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && (
    (context.custom has tokenIsLeveraged && context.custom.tokenIsLeveraged)
    || (context.custom has levClassPctAfter
        && context.custom.levClassPctAfter.greaterThan(decimal("10.0000")))
  )
};
