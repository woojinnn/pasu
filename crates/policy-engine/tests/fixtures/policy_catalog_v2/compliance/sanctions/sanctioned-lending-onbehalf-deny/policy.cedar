// blocks: a borrow taken on behalf of a sanctioned beneficiary (OFAC SDN / EU / UN).
// why:    sanctions screening — borrowing on behalf of a sanctioned party extends credit to a blocked beneficiary; screen the on-behalf-of borrower-of-record (credit-delegation / OFAC SLS).
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:lending
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the borrow `onBehalfOf` beneficiary against
// OFAC/EU/UN lists. `onBehalfOf` is optional — guarded with `has` before the
// sanctioned check. ENRICHMENT — dormant until the dispatcher serves the method.
@id("sanctioned-lending-onbehalf-deny")
@severity("deny")
@reason("Borrow beneficiary (onBehalfOf) is on a sanctions list (OFAC/EU/UN) — borrow blocked")
forbid(principal, action == Lending::Action::"Borrow", resource)
when {
  context has onBehalfOf
  && context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
