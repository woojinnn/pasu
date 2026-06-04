// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// scopeball EU compliance pack (INDIVIDUAL) — acquiring a NON-MiCA-authorized
// stablecoin (e-money token, e.g. USDT): cashout / liquidity protection.
//
// LEGAL BASIS (offeror-facing): Regulation (EU) 2023/1114 (MiCA), Art. 48(1):
//   "A person shall not make an offer to the public or seek the admission to
//   trading of an e-money token, within the Union, unless that person is the
//   issuer of such e-money token and: (a) is authorised as a credit institution
//   or as an electronic money institution; and (b) has notified a crypto-asset
//   white paper to the competent authority and has published that crypto-asset
//   white paper ..." Title IV (e-money tokens) applies from 30 June 2024; Title V
//   conduct rules (CASP offering of services in EMTs) apply from 30 December 2024.
// Authority: EBA / ESMA + national competent authorities.
//
// BINDS_INDIVIDUAL = INDIVIDUAL-PROTECTION (NOT a direct individual duty).
// Be brutally honest: MiCA Art. 48 restricts the ISSUER/OFFEROR and Title V
// restricts the CASP/VENUE — NEITHER prohibits a self-custody individual from
// HOLDING or acquiring a non-authorized EMT such as USDT. There is no personal
// legal duty here. What is REAL and concrete for the individual is the downstream
// harm: because EU-licensed venues that keep offering non-authorized EMTs risk
// their own MiCA authorization, every major EEA venue delisted non-authorized
// stablecoins for EEA users — Coinbase announced delisting on 3 Dec 2024 (USDT
// removal 31 Mar 2025), Kraken/Crypto.com/Binance followed by Q1 2025. So a
// self-custody EU resident who swaps INTO a non-authorized EMT faces a frozen
// cash-out path: EU venues may refuse the deposit / refuse to sell it back,
// stranding the position in illiquidity. This WARN is friction-avoidance, not a
// personal compliance duty — hence binds_individual = individual-protection and
// severity = warn (the individual is not breaking any law by acquiring it).
//
// JURISDICTION: residency is NOT on-chain; the EU pack is the install-time
// selector. "Authorized EMT issuer" is read against the EBA/ESMA EMT register,
// not via trigger.where.
//
// WHAT THIS DOES: on an AMM swap whose `tokenOut` is a fiat-referenced e-money
// token whose issuer is NOT MiCA-authorized, WARN before signing so the user
// understands the EEA cashout/liquidity risk. Host method
// external.mica_token_eligibility projects two flags into context.custom.
//
// EXPRESSIBILITY: needs-external (EBA/ESMA EMT authorization register). The call
// is `optional` (fail-open); `custom` is optional, so each flag is `has`-guarded
// before reading (has-guard triad). WARN fires only on a POSITIVE "is EMT AND
// issuer NOT authorized" reading, never on absence.
//
// ENFORCEMENT/IMPACT EXAMPLE: EEA users holding USDT on Kraken were moved to
// sell-only (27 Feb 2025) then had spot trading halted (24 Mar 2025); a self-
// custody holder who acquires USDT now must find an off-venue / OTC exit.
@id("eu-mica-nonauthorized-emt-acquire-warn")
@severity("warn")
@reason("The stablecoin you are about to receive is a non-MiCA-authorized e-money token; EU-licensed venues have delisted these, so your cash-out / deposit may be refused (Reg (EU) 2023/1114 Arts 48, Title V)")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has outTokenIsEmt
  && context.custom.outTokenIsEmt
  && context.custom has outTokenIssuerAuthorized
  && !context.custom.outTokenIssuerAuthorized
};
