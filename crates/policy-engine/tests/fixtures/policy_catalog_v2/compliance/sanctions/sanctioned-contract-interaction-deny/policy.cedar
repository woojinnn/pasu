// blocks: interacting with a contract that is itself on the OFAC SDN list (e.g. Tornado Cash / Blender / Sinbad).
// why:    sanctions screening — calling a sanctioned contract (an SDN-listed mixer/protocol) is a direct compliance violation; the Unknown catch-all is where un-decoded calldata to such a target lands (Treasury SDN-listed contracts).
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:unknown
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the call target against OFAC/EU/UN lists. Covers
// SDN-listed contracts (Tornado/Blender/Sinbad). ENRICHMENT — dormant until wired.
@id("sanctioned-contract-interaction-deny")
@severity("deny")
@reason("Target contract is on the OFAC SDN list (e.g. a sanctioned mixer) — interaction blocked")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
