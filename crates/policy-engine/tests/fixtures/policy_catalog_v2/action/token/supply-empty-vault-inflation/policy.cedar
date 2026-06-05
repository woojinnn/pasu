// blocks: a supply/deposit into a near-empty market/vault where an early depositor can be diluted by a share-price inflation (donation) attack.
// why:    ERC-4626 vault first-depositor / share-inflation (donation) attack threat class — a tiny totalSupply lets an attacker round the victim's minted shares to zero by donating assets to skew share price.
// bucket: action/token  (precedence-won)
// tags: bucket:action cat:token enrichment method:vault.share_state
// methods: vault.share_state -> ../../../_methods/vault.share_state.md
//
// Catalog I3 — warn when supplying into a near-empty market/vault; vault.share_state enriches the inflation-risk flag.
// ENRICHMENT — dormant until the dispatcher serves `vault.share_state`.
@id("supply-empty-vault-inflation")
@severity("warn")
@reason("This market/vault is nearly empty — early deposits risk a share-inflation attack")
forbid(principal, action == Lending::Action::"Supply", resource)
when {
  context has custom
  && context.custom has inflationRisk
  && context.custom.inflationRisk == true
};
