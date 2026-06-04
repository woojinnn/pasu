// scopeball default v2 policy — ERC20 approve to the Morpho Bundler3 CORE
// (access-control-less) instead of an adapter.
// Source: policy-factory cycle 2026-06-03-morpho (protocol scope = Morpho only).
// Demand:
//   - Morpho App incident Apr-10-2025: approvals routed "to the Bundler3 contract
//     and not the respective adapters" (morpho.org/blog/morpho-app-incident-...);
//     "Bundler3 was not designed to enforce access controls (unlike its adapters),
//     this opened a window for malicious actors" -> mempool front-run, ~$2.6M at
//     risk, white-hat recovered (quadrigainitiative case study).
//   - ScamSniffer/SEAL: Morpho the #2 most-impersonated brand (31%) in a 2026
//     Google-ads phishing wave (coingabbar, 2026-05-26).
//
// EXTERNAL/REGISTRY. The danger is a specific spender address: the Morpho
// Bundler3 core entrypoint (no access control) rather than a known access-
// controlled adapter. cedar cannot know contract roles; a registry/external
// classifier sets context.custom.spenderIsBundlerCore. Fires on any erc20_approve
// (token actions carry no venue); the has-guard makes it a no-op unless the
// classifier flags the spender. WARN, fail-open.
@id("morpho-approve-to-bundler-core-warn")
@severity("warn")
@reason("You're approving the Morpho bundler entry contract, which has no access control — approvals should target an access-controlled adapter (a mempool front-run of this exact mistake put ~$2.6M at risk in Apr 2025)")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has spenderIsBundlerCore
  && context.custom.spenderIsBundlerCore
};
