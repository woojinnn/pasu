// blocks: proceeding when the sanctions list backing the screen is stale (could not be confirmed fresh).
// why:    fail-on-stale meta-policy — a sanctions screen is only meaningful against a current list; if the provider reports the list as stale (sanctionsFresh == false), the screen cannot be trusted, so warn the user that screening is degraded rather than silently passing (OFAC SLS freshness guidance).
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:unknown
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` projects a `sanctionsFresh` freshness flag; this rule
// warns when the list is NOT fresh (false), i.e. screening is degraded. Note the
// `== false` only fires once the flag is present (host populated it as false) —
// an absent flag leaves the policy inert. ENRICHMENT — dormant until wired.
@id("sanctions-list-stale-fail-warn")
@severity("warn")
@reason("Sanctions list is stale — screening could not be confirmed against a current list; proceed with caution")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has sanctionsFresh
  && context.custom.sanctionsFresh == false
};
