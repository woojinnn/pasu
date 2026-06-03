// blocks: completing an EigenLayer withdrawal whose `withdrawer` (the recipient of the released shares/tokens) is not you.
// why:    `completeQueuedWithdrawal` releases the queued shares/tokens to the withdrawer recorded at queue time; if that withdrawer is not the submitter, the funds land in a third party (fund-redirect / withdrawer-mismatch threat class). Source: EigenLayer DelegationManager docs.
// bucket: protocol/eigenlayer
// tags: bucket:protocol cat:eigenlayer static venue:eigenlayer action-family:restaking
//
// Pure-static. `withdrawer` is a required field (VOCAB §4 restaking
// CompleteWithdrawal); compare to `meta.submitter`.
@id("eigenlayer-complete-withdrawal-to-other")
@severity("warn")
@reason("Withdrawal completes to a withdrawer that is not you — confirm the funds should go to this other account")
forbid(principal, action == Restaking::Action::"CompleteWithdrawal", resource)
when {
  context.withdrawer != context.meta.submitter
};
