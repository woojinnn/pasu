// blocks: an AMM swap whose output recipient is on an OFAC SDN / EU / UN sanctions list.
// why:    sanctions screening — routing swap proceeds to a sanctioned recipient launders value to a blocked party; screen the output recipient just like a direct transfer (OFAC SLS / Treasury SDN list).
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:swap
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the swap output recipient against OFAC/EU/UN lists.
// ENRICHMENT — dormant until the dispatcher serves the method.
@id("sanctioned-swap-recipient-deny")
@severity("deny")
@reason("Swap output recipient is on a sanctions list (OFAC/EU/UN) — swap blocked")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
