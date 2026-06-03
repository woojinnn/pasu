// blocks: a launchpad sale Commit whose allocation recipient is on an OFAC SDN / EU / UN sanctions list.
// why:    sanctions screening — directing a launchpad commitment/allocation to a legally-sanctioned address is a compliance violation; the allocation recipient is a screenable destination. Same OFAC/EU/UN feed, new bind-site (Launchpad Commit.recipient). (OFAC SDN list / EU / UN sanctions.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:commit
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the commit allocation recipient against OFAC/EU/UN lists.
// ENRICHMENT — dormant until the dispatcher serves the method; the
// `context.custom has sanctioned` guard is false (inert) until then.
@id("sanctioned-launchpad-recipient-deny")
@severity("deny")
@reason("Launchpad commit recipient is on a sanctions list (OFAC/EU/UN) — commit blocked")
forbid(principal, action == Launchpad::Action::"Commit", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
