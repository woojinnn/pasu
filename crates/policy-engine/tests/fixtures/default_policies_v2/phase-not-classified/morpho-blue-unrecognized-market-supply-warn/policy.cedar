// scopeball default v2 policy — Morpho Blue permissionless market not recognized.
// Source: policy-factory cycle 2026-06-03-morpho (protocol scope = Morpho only).
// Demand:
//   - PAXG/USDC Morpho Blue market drained ~$230K (Oct-2024) — "Oracle
//     SCALE_FACTOR was misconfigured, failing to account for the differences
//     between decimals in USDC (6) and PAXG (18)" (cryptopolitan, 2024-10-14).
//   - Morpho's OWN risk-warning interface: "As anyone can create a market/vault,
//     there is no guarantee that every market/vault on the interface is safe"
//     (morpho.org/blog/introducing-risk-warnings..., 2024-05-02).
//   - Morpho docs: "Supplying liquidity in a market with a faulty oracle (one
//     that overestimates the collateral's price) will lead to a loss of assets"
//     (docs.morpho.org/curate/concepts/security-considerations).
//   - X: "Morpho lets anyone create isolated markets with custom collateral like
//     volatile meme coins, risk parameters, and oracles" (.../2060386464089641414).
//
// EXTERNAL. Morpho Blue is permissionless: market_id = keccak(loan, collateral,
// oracle, irm, lltv) and ANY oracle/collateral/LLTV is allowed with no
// governance vetting. The wallet cannot judge a market from calldata, and the
// market_id is an opaque bytes32 hash. An external resolver maps the venue's
// market_id -> params and checks the oracle + collateral against Morpho's
// recognized allowlist (the same RED-warning list Morpho's own UI uses),
// projecting one Bool into context.custom.marketNotRecognized. Unbuilt feed ->
// optional/fail-open (the has-guard triad makes absence a silent no-op). WARN,
// never deny on an external signal.
@id("morpho-blue-unrecognized-market-supply-warn")
@severity("warn")
@reason("This Morpho Blue market is not on the recognized list — its oracle or collateral is unvetted and your supplied funds could be lost")
forbid(principal, action == Lending::Action::"Supply", resource)
when {
  context has custom
  && context.custom has marketNotRecognized
  && context.custom.marketNotRecognized
};
