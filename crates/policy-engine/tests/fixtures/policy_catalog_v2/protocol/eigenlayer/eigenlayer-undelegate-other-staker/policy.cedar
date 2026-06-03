// blocks: force-undelegating a staker that is NOT you (the EigenLayer staker field differs from the submitter).
// why:    `undelegate(staker)` is callable for a staker other than the caller (operator or operator's approver path); a submission whose `staker` is not the signer means you are undelegating someone else's position — confirm it is intended (unexpected-target / third-party-effect threat class). Source: EigenLayer DelegationManager#1247.
// bucket: protocol/eigenlayer
// tags: bucket:protocol cat:eigenlayer static venue:eigenlayer action-family:restaking
//
// Pure-static. `staker` is a required field (VOCAB §4 restaking Undelegate);
// `meta.submitter` is the actor — flag a mismatch.
@id("eigenlayer-undelegate-other-staker")
@severity("warn")
@reason("Undelegating a staker that is not you — confirm you intend to undelegate this other account")
forbid(principal, action == Restaking::Action::"Undelegate", resource)
when {
  context.staker != context.meta.submitter
};
