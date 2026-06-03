// blocks: an EigenLayer-style restaking DelegateTo whose operator is on an OFAC SDN / EU / UN sanctions list.
// why:    sanctions screening — delegating restaked stake to a legally-sanctioned operator routes economic weight (and slashing/reward exposure) to a sanctioned party; the operator is a screenable counterparty. Same OFAC/EU/UN feed, new bind-site (DelegateTo.operator). (OFAC SDN list / EU / UN sanctions.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:delegate
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the delegation operator against OFAC/EU/UN lists.
// ENRICHMENT — dormant until the dispatcher serves the method; the
// `context.custom has sanctioned` guard is false (inert) until then.
@id("sanctioned-restaking-operator-deny")
@severity("deny")
@reason("Restaking operator is on a sanctions list (OFAC/EU/UN) — delegation blocked")
forbid(principal, action == Restaking::Action::"DelegateTo", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
