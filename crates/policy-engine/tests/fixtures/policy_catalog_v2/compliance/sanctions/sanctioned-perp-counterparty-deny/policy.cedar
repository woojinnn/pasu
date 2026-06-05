// blocks: opening a perp position whose collateral asset is itself a sanctioned (SDN-listed) token contract.
// why:    sanctions screening — posting collateral denominated in a sanctioned token contract routes value through a blocked instrument; screen the collateral token address as the perp bind-site (OpenPosition exposes no free-form counterparty address). (OFAC SLS / Treasury SDN list.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:perp
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the perp collateral token contract against
// OFAC/EU/UN lists. ENRICHMENT — dormant until the dispatcher serves the method.
@id("sanctioned-perp-counterparty-deny")
@severity("deny")
@reason("Perp collateral token is a sanctioned (SDN-listed) contract — position open blocked")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
