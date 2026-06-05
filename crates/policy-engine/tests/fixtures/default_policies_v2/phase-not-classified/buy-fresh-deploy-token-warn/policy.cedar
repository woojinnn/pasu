// scopeball default v2 policy — contract-AGE risk on a token BUY (Amm::Swap;
// tokenOut is the asset being acquired).
//
// A tokenOut whose contract was deployed only seconds/minutes ago is the
// classic snipe-rug / honeypot setup: honeypot-as-a-service kits mint a fresh
// token, seed thin liquidity, let retail buy, then pull within 24h (Chainalysis
// /TRM: rug pulls ≈ $3-4B/yr, the largest retail-facing scam category;
// DEXTools 2026: token creation in minutes, rug even faster). This is a DISTINCT
// axis from honeypot detection (swap-out-token-honeypot-warn), LP/owner power
// (buy-rug-risk-lp-owner-power-warn) and hidden-mint/proxy power — none of those
// inspect DEPLOY TIME.
//
// `tokenOut` may be absent (sell-only / native-out swaps); the fact method
// handles that and emits no age, so the call is `optional` and fail-opens. The
// Long is projected into context.custom.outTokenAgeSeconds and 3-guarded before
// the native `<` comparison. 86400s = 24h. ENRICHMENT (needs-method) — dormant
// until token.out_contract_age_seconds is served.
@id("buy-fresh-deploy-token-warn")
@severity("warn")
@reason("The token you are about to receive was deployed very recently — fresh-deploy tokens are a common snipe-rug or honeypot setup")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has outTokenAgeSeconds
  && context.custom.outTokenAgeSeconds < 86400
};
