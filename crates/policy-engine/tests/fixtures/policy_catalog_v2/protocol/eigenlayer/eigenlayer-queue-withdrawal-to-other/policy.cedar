// blocks: queueing an EigenLayer withdrawal whose `withdrawer` (who may later complete + claim the shares) is not you.
// why:    `queueWithdrawals` records the withdrawer that is entitled to complete the withdrawal and receive the released shares/tokens; setting it to a third party pre-commits your exiting stake to that account (fund-redirect / withdrawer-mismatch threat class). Source: EigenLayer DelegationManager docs.
// bucket: protocol/eigenlayer
// tags: bucket:protocol cat:eigenlayer static venue:eigenlayer action-family:restaking
//
// Pure-static. `withdrawer` is a required field (VOCAB §4 restaking
// QueueWithdrawal); compare to `meta.submitter`.
@id("eigenlayer-queue-withdrawal-to-other")
@severity("warn")
@reason("Withdrawal is queued to a withdrawer that is not you — confirm this other account should be able to claim it")
forbid(principal, action == Restaking::Action::"QueueWithdrawal", resource)
when {
  context.withdrawer != context.meta.submitter
};
