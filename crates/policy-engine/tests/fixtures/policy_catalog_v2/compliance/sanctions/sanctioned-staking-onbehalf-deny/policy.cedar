// blocks: a vote-escrow IncreaseLockAmount (Curve veCRV deposit_for) whose on-behalf-of beneficiary is on an OFAC SDN / EU / UN sanctions list.
// why:    sanctions screening — topping up a lock on behalf of a legally-sanctioned beneficiary commits value to a sanctioned party's escrow; the `onBehalfOf` beneficiary is a screenable counterparty. Same OFAC/EU/UN feed, new bind-site (IncreaseLockAmount.onBehalfOf). (OFAC SDN list / EU / UN sanctions.)
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true action-family:stake
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the `onBehalfOf` beneficiary against OFAC/EU/UN lists.
// `onBehalfOf` is OPTIONAL (omitted ⇒ submitter), so the guard first checks
// `context has onBehalfOf`. ENRICHMENT — dormant until the dispatcher serves the
// method; the `context.custom has sanctioned` guard is false (inert) until then.
@id("sanctioned-staking-onbehalf-deny")
@severity("deny")
@reason("veCRV deposit_for beneficiary is on a sanctions list (OFAC/EU/UN) — lock top-up blocked")
forbid(principal, action == Staking::Action::"IncreaseLockAmount", resource)
when {
  context has onBehalfOf
  && context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
