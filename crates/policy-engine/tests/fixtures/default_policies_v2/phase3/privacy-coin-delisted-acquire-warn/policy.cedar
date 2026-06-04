// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// scopeball JP individual pack — INDIVIDUAL-PROTECTION.
//
// Since 2018 Japan's FSA pressured all registered exchanges to drop the major
// privacy / "anonymity" coins — Monero (XMR), Zcash (ZEC), Dash (DASH) — as
// untraceable AML risks; Coincheck delisted XMR/ZEC/DASH/REP in June 2018 and
// no FSA-registered (JVCEA-member) exchange lists them today. This is NOT a
// personal legal duty: a self-custody individual breaks no Japanese law by
// merely holding XMR. It is PROTECTION against a concrete, foreseeable harm —
// the user who swaps INTO a JP-delisted privacy coin has NO domestic on-ramp/
// off-ramp to ever cash it back to JPY through a licensed exchange, and routing
// such coins risks AML-flagged deposits / account freezes when they later try
// to return value to a Japanese venue. So this warns the individual BEFORE the
// swap, when the position is still avoidable.
//
// The acquired token (`tokenOut`) is typically UNHELD, so its identity must be
// classified externally: the manifest calls token.swap_out_classification
// against the JP-delisted-privacy registry and projects the hit into
// context.custom.outTokenJpDelistedPrivacy. The call is `optional` (fail-open
// until the registry ships) and `custom` is optional, so the flag is
// `has`-guarded before reading.
@id("privacy-coin-delisted-acquire-warn")
@severity("warn")
@reason("This token is a privacy coin (e.g. XMR/ZEC/DASH) delisted by all FSA-registered Japanese exchanges — you may be unable to cash it out and risk AML-frozen deposits")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has outTokenJpDelistedPrivacy
  && context.custom.outTokenJpDelistedPrivacy
};
