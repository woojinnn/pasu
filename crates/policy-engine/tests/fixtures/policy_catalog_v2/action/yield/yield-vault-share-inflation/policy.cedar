// blocks: adding liquidity / minting LP into a near-empty Pendle market where an early provider can be diluted by a share-price inflation (donation) attack.
// why:    ERC-4626-style first-depositor / share-inflation threat class — a tiny LP supply lets an attacker donate assets to skew share price and round the victim's minted LP toward zero. Source: EIP-4626 inflation-attack discussion.
// bucket: action/yield
// tags: bucket:action cat:yield enrichment method:vault.share_state
// methods: vault.share_state -> ../../../_methods/vault.share_state.md
// dormant: until vault.share_state is registered in method-catalog.json
//
// ENRICHMENT — `context.custom.inflationRisk` is host-populated by vault.share_state;
// always `has`-guarded so the policy is inert until the method is wired.
@id("yield-vault-share-inflation")
@severity("warn")
@reason("This market is nearly empty — early liquidity risks a share-inflation attack")
forbid(principal, action == Yield::Action::"AddMarketLiquidity", resource)
when {
  context has custom
  && context.custom has inflationRisk
  && context.custom.inflationRisk == true
};
