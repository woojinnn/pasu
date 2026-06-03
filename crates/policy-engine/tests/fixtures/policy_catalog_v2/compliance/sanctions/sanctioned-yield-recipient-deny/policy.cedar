// blocks: a Pendle-style yield PtSwap whose output recipient is on an OFAC SDN / EU / UN sanctions list.
// why:    sanctions screening — routing PT-swap output (value) to a legally-sanctioned address is a compliance violation and an irreversible loss; the swap recipient is a screenable destination. Same OFAC/EU/UN feed, new bind-site (PtSwap.recipient). (OFAC SDN list / EU / UN sanctions.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:swap
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the PT-swap output recipient against OFAC/EU/UN lists.
// ENRICHMENT — dormant until the dispatcher serves the method; the
// `context.custom has sanctioned` guard is false (inert) until then.
@id("sanctioned-yield-recipient-deny")
@severity("deny")
@reason("PT-swap recipient is on a sanctions list (OFAC/EU/UN) — swap blocked")
forbid(principal, action == Yield::Action::"PtSwap", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
