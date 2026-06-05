// blocks: claiming a finalized Lido withdrawal where the released ETH is sent to an address other than the signer.
// why:    output-redirection — `claimWithdrawalsTo(requestIds, hints, recipient)` releases the finalized ETH to `recipient`; an injected `recipient` sends the claimed ETH to a third party (fund-redirect threat class). Source: Lido WithdrawalQueue `claimWithdrawalsTo` docs.
// bucket: protocol/lido
// tags: bucket:protocol cat:lido static venue:lido action-family:recipient-not-self
//
// Pure-static. `recipient` is OPTIONAL (VOCAB §4 liquid_staking ClaimWithdrawal,
// absent == claim to self) — `has`-guard before the mismatch compare.
@id("lido-claim-withdrawal-recipient-not-self")
@severity("warn")
@reason("This withdrawal claim releases the ETH to another address — confirm the recipient before signing")
forbid(principal, action == LiquidStaking::Action::"ClaimWithdrawal", resource)
when {
  context has recipient
  && context.recipient != context.meta.submitter
};
