// scopeball default v2 policy — Token Due Diligence Set: buying a token whose
// contract carries hidden privileged powers that a "verified" badge hides.
//
// An Etherscan/explorer "verified" source badge proves the *displayed* source
// compiles to the deployed bytecode — it does NOT prove the token is safe. A
// verified contract can still expose owner-only mint, blacklist, pausable, or
// owner-only-transfer functions, and a verified *proxy* can point at an
// upgradable implementation whose malicious logic is swapped in after launch
// (the badge sits on the proxy, the danger sits behind it). Documented SCAM
// labels on verified-looking contracts (trustchain, tokenchecker, Etherscan).
//
// This is sv = needs-external: the privileged-function surface and proxy
// upgradability come from static bytecode/source analysis of `tokenOut`, made
// outside wallet state. The method `external.token_contract_powers` folds the
// probe and emits one Bool per power, projected into `context.custom`. The call
// is `optional` (fail-open until the analyzer ships) and `custom` is optional,
// so each flag is `has`-guarded before reading. Warns — the powers may be
// benign, but the user should look before signing a blind buy.
@id("buy-hidden-mint-proxy-power-warn")
@severity("warn")
@reason("The token you are buying has hidden privileged powers (mint/blacklist/pausable/owner-only-transfer or an upgradable proxy) — a 'verified' badge does not make these safe")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && (
    (context.custom has outTokenHasMintPower && context.custom.outTokenHasMintPower)
    || (context.custom has outTokenHasBlacklistPower && context.custom.outTokenHasBlacklistPower)
    || (context.custom has outTokenHasPausablePower && context.custom.outTokenHasPausablePower)
    || (context.custom has outTokenHasOwnerOnlyTransfer && context.custom.outTokenHasOwnerOnlyTransfer)
    || (context.custom has outTokenProxyUpgradable && context.custom.outTokenProxyUpgradable)
  )
};
