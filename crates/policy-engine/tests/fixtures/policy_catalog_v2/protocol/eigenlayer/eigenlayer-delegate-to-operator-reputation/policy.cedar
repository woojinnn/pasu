// blocks: delegating all your restaked shares to an EigenLayer operator already flagged as known-malicious (scam / drainer / abusive operator).
// why:    EigenLayer `delegateTo` hands the operator slashing authority over your entire restaked stake; a flagged operator can route your stake into a slashable/abusive posture (stake-custody / slashing-authority threat class). Source: EigenLayer DelegationManager docs.
// bucket: protocol/eigenlayer
// tags: bucket:protocol cat:eigenlayer enrichment method:address.reputation venue:eigenlayer could-be-compliance:reputation-feed action-family:restaking
// methods: address.reputation -> ../../../_methods/address.reputation.md
// dormant: until address.reputation is registered in method-catalog.json
//
// ENRICHMENT — `context.custom.operatorFlagged` is host-populated by
// address.reputation on the target operator; always `has`-guarded so the policy
// is inert until the method is wired.
@id("eigenlayer-delegate-to-operator-reputation")
@severity("warn")
@reason("The EigenLayer operator is flagged as a known-malicious address — confirm before delegating your restaked shares")
forbid(principal, action == Restaking::Action::"DelegateTo", resource)
when {
  context has custom
  && context.custom has operatorFlagged
  && context.custom.operatorFlagged == true
};
