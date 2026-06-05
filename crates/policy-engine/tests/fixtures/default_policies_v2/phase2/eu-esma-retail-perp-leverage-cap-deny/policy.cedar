// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// scopeball EU compliance pack — ESMA retail leverage cap (crypto product
// intervention), applied to on-chain perpetuals.
//
// LEGAL BASIS: ESMA Decision (EU) 2018/796 of 22 May 2018 (OJ L 136, 1.6.2018),
// adopted under Article 40 of Regulation (EU) No 600/2014 (MiFIR), restricting
// the marketing, distribution or sale of CFDs to retail clients. It sets leverage
// limits "on the opening of a position by a retail client" ranging from 30:1 to
// 2:1 by underlying volatility — and 2:1 for CFDs on cryptocurrencies (the most
// restrictive tier), applicable from 1 August 2018. The ESMA temporary measure
// was made permanent at national level by the NCAs (e.g. BaFin, AMF) mirroring
// its terms. ESMA has since confirmed (public statement ESMA35-243228190-8024)
// that perpetual futures offered to retail clients fall within the scope of these
// CFD product-intervention measures.
//
// WHAT THIS DOES (compliance-assist, NOT legal advice): on opening a perpetual
// position, this DENYs an order whose effective leverage exceeds the EU retail
// crypto cap of 2:1 before signing. A self-custody retail trader who installs the
// EU pack thus keeps on-chain perp leverage within the limit EU-regulated venues
// must enforce for retail clients on crypto exposures.
//
// JURISDICTION + CLIENT CLASS: retail-client status and residency are NOT on-chain
// fields. The user opts into the EU retail tier by installing the EU bundle; the
// 2x cap is the install-time retail parameter (a professional-client variant of
// the pack would carry a different cap), NOT in trigger.where (see why_install).
//
// EXPRESSIBILITY: needs-method. Order leverage cannot be read numerically from the
// base context — `context.leverage` and the `leverage_implied` SizeSpec arm are
// raw Decimal-as-String. The host method `perp.order_leverage` resolves the
// effective leverage (explicit, or collateral×size-implied) into the Cedar
// `decimal` `orderLeverage`, compared with `.greaterThan(...)`. The call is
// `optional` (fail-open) and `custom` is optional, so the field is `has`-guarded
// (has-guard triad).
@id("eu-esma-retail-perp-leverage-cap-deny")
@severity("deny")
@reason("This order's leverage exceeds the EU retail crypto cap of 2:1 (ESMA Decision (EU) 2018/796, made permanent by national regulators)")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has orderLeverage
  && context.custom.orderLeverage.greaterThan(decimal("2.0000"))
};
