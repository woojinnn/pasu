// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// scopeball EU compliance pack — MiCA non-authorized stablecoin (e-money token).
//
// LEGAL BASIS: Regulation (EU) 2023/1114 (Markets in Crypto-Assets, "MiCA"),
// Article 48(1): "A person shall not make an offer to the public or seek the
// admission to trading of an e-money token, within the Union, unless that person
// is the issuer of such e-money token and: (a) is authorised as a credit
// institution or as an electronic money institution; and (b) has notified a
// crypto-asset white paper to the competent authority and has published that
// crypto-asset white paper in accordance with Article 51." Title IV (e-money
// tokens) has applied since 30 June 2024. EU-served venues delisted non-compliant
// stablecoins (most prominently Tether USDT) for EEA users from Dec 2024–Mar 2025
// to stay within MiCA Title V conduct rules.
//
// WHAT THIS DOES (compliance-assist, NOT legal advice): on an AMM swap whose
// `tokenOut` is a fiat-referenced e-money token (stablecoin) whose issuer is NOT
// MiCA-authorized (no authorised EMT issuer + published white paper in the ESMA /
// EBA register), this DENYs the swap before signing. A self-custody trader who
// installs the EU pack thereby avoids acquiring a stablecoin that EU-authorized
// venues may refuse to take back, that cannot lawfully be offered/admitted to
// trading to the EU public, and that carries MiCA conduct exposure.
//
// JURISDICTION: residency is NOT on-chain; the user opts in by installing the EU
// bundle. "Authorized" is judged against the EU EMT register (install-time pack
// selector), not via trigger.where (see why_install).
//
// EXPRESSIBILITY: needs-external. Whether `tokenOut` is an EMT and whether its
// issuer holds MiCA authorization is an external register read
// (external.mica_token_eligibility against the ESMA/EBA EMT register). The call is
// `optional` (fail-open) — if the register is unreachable the policy no-ops rather
// than SystemFail. `custom` is optional, so each flag is `has`-guarded before
// reading (has-guard triad). DENY fires only on a POSITIVE "is EMT AND issuer not
// authorized" reading, never on absence.
@id("eu-mica-nonauthorized-emt-acquire-deny")
@severity("deny")
@reason("The stablecoin you are about to receive is an e-money token whose issuer is not MiCA-authorized (Reg (EU) 2023/1114 Art 48); EU-authorized venues may refuse it")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has outTokenIsEmt
  && context.custom.outTokenIsEmt
  && context.custom has outTokenIssuerAuthorized
  && !context.custom.outTokenIssuerAuthorized
};
