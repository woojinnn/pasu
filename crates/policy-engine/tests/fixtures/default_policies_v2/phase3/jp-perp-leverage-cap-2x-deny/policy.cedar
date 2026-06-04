// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// ScopeBall JP compliance pack — retail crypto-asset margin leverage cap (2x).
//
// REGIME: For crypto-asset margin transactions by an INDIVIDUAL (retail), the
// Cabinet Office Order on Cryptoasset Exchange Service Providers (暗号資産交換業者
// に関する内閣府令 / Cabinet Office Order No. 7 of 2017), Article 25, paragraph (5),
// item (i), requires the security deposit to be at least 50% of the transaction
// amount — i.e. a maximum leverage of 2x. The regime sits under the Payment
// Services Act / Financial Instruments and Exchange Act framework (2x cap in
// force since the May 2020 revision); JVCEA is the designated self-regulatory
// organization (SRO) detailing it. Domestic registered venues hard-cap at 2x;
// the live compliance gap is a self-custody trader opening >2x on an OFFSHORE /
// DEX perp venue not registered in Japan.
//
// WHAT THIS DOES: deny an `open_position` whose effective leverage exceeds the
// 2x legal cap. Leverage CANNOT be read in Cedar from the base context
// (`context.leverage` / `context.size.leverage` are raw Decimal-as-String and
// Cedar cannot compare a String numerically). The manifest's `perp.order_leverage`
// method resolves effective leverage (explicit, or collateral×size-implied) into
// the enriched `decimal` `orderLeverage`. Threshold is the LEGAL 2x (decimal
// "2.0000"), not a user preference.
//
// FAIL-OPEN: `perp.order_leverage` is defined-not-yet-implemented, so `optional`
// (fail-open). `custom` is optional, so each path is `has`-guarded. COMPLIANCE
// ASSIST, not legal advice — the binding rule is the Cabinet Office Order.
@id("jp-perp-leverage-cap-2x-deny")
@severity("deny")
@reason("Leverage exceeds Japan's 2x retail crypto-asset margin cap (Cabinet Office Order on Cryptoasset Exchange Service Providers, Art. 25(5)(i) — 50% minimum margin)")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has orderLeverage
  && context.custom.orderLeverage.greaterThan(decimal("2.0000"))
};
