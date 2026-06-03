// blocks: an un-decoded interaction whose counterparty (call target) is on a sanctions list — soft warn.
// why:    sanctions screening — a catch-all warn tier for any un-decoded call whose target screens as sanctioned, so the user is alerted even when no specific decoder matched. Warn (not deny) because the Unknown path lacks semantic context to justify a hard block. (OFAC SLS / Treasury SDN list.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:unknown
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the Unknown call target against OFAC/EU/UN lists.
// Warn-tier sibling of sanctioned-contract-interaction-deny. ENRICHMENT —
// dormant until the dispatcher serves the method.
@id("sanctioned-counterparty-warn")
@severity("warn")
@reason("This interaction's counterparty is on a sanctions list (OFAC/EU/UN) — review before proceeding")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
