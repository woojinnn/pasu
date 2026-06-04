// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// ScopeBall US compliance pack — CFTC retail leveraged-commodity exposure
// (pre-sign awareness).
//
// REGIME: Commodity Exchange Act (CEA) section 2(c)(2)(D), 7 U.S.C. 2(c)(2)(D).
// A retail commodity transaction — a transaction in a commodity (the CFTC treats
// Bitcoin/virtual currencies as commodities) offered to a person who is NOT an
// "eligible contract participant" (an individual ECP needs >$10M invested on a
// discretionary basis, CEA 1a(18) / 7 U.S.C. 1a(18)) on a LEVERAGED, margined or
// financed basis — is regulated "as if" a futures contract and must be conducted
// on a CFTC-registered Designated Contract Market (DCM), UNLESS it results in
// "actual delivery" within 28 days (CEA 2(c)(2)(D)(ii)(III)(aa)). Offering such
// leveraged retail trading off a registered venue is unlawful (see BitMEX, $100M,
// Aug 10 2021).
//
// WHY PRE-SIGN, SELF-CUSTODY ANGLE: a US retail trader (non-ECP) opening a
// leveraged perp on an offshore/unregistered venue is the exact conduct CEA
// 2(c)(2)(D) governs. This jurisdiction pack WARNs at the sign gate when an open
// carries retail leverage (here > 1x flags any margined/leveraged retail open;
// the cap is install-time configurable) so the US trader is alerted that leveraged
// retail commodity trading belongs on a CFTC-registered DCM — a control a generic
// simulator cannot supply.
//
// HONESTY / [verify]: the CFTC's 2020 FINAL INTERPRETIVE GUIDANCE on "actual
// delivery" (85 FR 37734, eff. June 24 2020) was WITHDRAWN effective Dec 10 2025
// (90 FR, withdrawal notice). The underlying STATUTE (CEA 2(c)(2)(D) and the ECP
// definition) remains in force; only the interpretive gloss on the 28-day
// delivery exception was withdrawn. This pack is therefore a WARN (conduct alert),
// not a deny, and is compliance-ASSIST, not legal advice.
//
// MECHANISM: extends the shipped `perp-leverage-cap-deny` pattern. Order leverage
// cannot be read numerically from the base context (raw Decimal-as-String); the
// `perp.order_leverage` policy_rpc call resolves effective leverage into the Cedar
// `decimal` `context.custom.orderLeverage`. `custom` and the field are
// `has`-guarded; the call is `optional` (fail-open until the resolver ships).
//
// ScopeBall assists compliance; it is not legal advice.
@id("cftc-retail-leverage-cap-warn")
@severity("warn")
@reason("Leveraged retail crypto trade: CFTC CEA 2(c)(2)(D) requires leveraged retail commodity trading on a registered DCM (or actual delivery within 28 days)")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has orderLeverage
  && context.custom.orderLeverage.greaterThan(decimal("1.0000"))
};
