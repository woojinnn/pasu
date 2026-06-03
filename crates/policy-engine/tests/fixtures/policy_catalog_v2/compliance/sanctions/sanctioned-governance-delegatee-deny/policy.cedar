// blocks: a governance voting-power Delegate whose delegatee is on an OFAC SDN / EU / UN sanctions list.
// why:    sanctions screening — delegating governance voting weight to a legally-sanctioned address hands protocol influence to a sanctioned party; the delegatee is a screenable counterparty. Same OFAC/EU/UN feed, new bind-site (Airdrop Delegate.delegatee). (OFAC SDN list / EU / UN sanctions.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:delegate
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the governance delegatee against OFAC/EU/UN lists.
// ENRICHMENT — dormant until the dispatcher serves the method; the
// `context.custom has sanctioned` guard is false (inert) until then.
@id("sanctioned-governance-delegatee-deny")
@severity("deny")
@reason("Governance delegatee is on a sanctions list (OFAC/EU/UN) — delegation blocked")
forbid(principal, action == Airdrop::Action::"Delegate", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
