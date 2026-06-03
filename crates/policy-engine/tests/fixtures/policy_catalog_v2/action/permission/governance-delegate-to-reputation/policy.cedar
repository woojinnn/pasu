// blocks: delegating governance voting power to a delegatee already flagged as malicious (vote-buying / governance-capture bait).
// why:    ERC20Votes `delegate(address)` hands the user's full voting weight to the delegatee; a flagged delegatee can aggregate weight to capture a protocol's governance (governance-attack threat class). Source: OpenZeppelin Governor / ERC20Votes docs.
// bucket: action/permission
// tags: bucket:action cat:permission enrichment method:address.reputation could-be-compliance:reputation-feed action-family:governance
// methods: address.reputation -> ../../../_methods/address.reputation.md
// dormant: until address.reputation is registered in method-catalog.json
//
// ENRICHMENT — `context.custom.delegateeFlagged` is host-populated by
// address.reputation on the delegatee; always `has`-guarded so the policy is
// inert until the method is wired.
@id("governance-delegate-to-reputation")
@severity("warn")
@reason("The delegatee is flagged as a known-malicious address — confirm before handing over your voting power")
forbid(principal, action == Airdrop::Action::"Delegate", resource)
when {
  context has custom
  && context.custom has delegateeFlagged
  && context.custom.delegateeFlagged == true
};
