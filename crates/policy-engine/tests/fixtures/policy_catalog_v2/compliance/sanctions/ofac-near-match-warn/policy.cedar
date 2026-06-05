// blocks: interacting with a target that a graph-exposure provider flags as proximate to a sanctioned address — soft warn.
// why:    sanctions screening (proximity) — most free SDN feeds are exact-match only; a near-match / one-hop-exposure flag is provider-dependent and probabilistic, so this is a WARN, not a deny. Honest framing per OFAC FAQ 1641 (no strict-liability mandate from a proximity heuristic). (OFAC FAQ 1641.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:unknown
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` here carries a provider-dependent proximity/near-match
// flag projected into `sanctioned`. Probabilistic → warn tier. ENRICHMENT —
// dormant until the dispatcher serves the method.
@id("ofac-near-match-warn")
@severity("warn")
@reason("Target is flagged as proximate to a sanctioned address (provider near-match) — review before proceeding")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
