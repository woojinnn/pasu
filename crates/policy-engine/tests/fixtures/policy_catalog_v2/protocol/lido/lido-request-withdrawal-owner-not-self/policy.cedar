// blocks: a Lido withdrawal request whose `owner` (the address credited with the withdrawal NFT / claimable ETH) is not you.
// why:    owner-redirect — `requestWithdrawals(amounts, owner)` mints the unstETH withdrawal request to `owner`; if a malicious frontend injects an `owner` other than the signer, the eventual claimable ETH accrues to a third party (fund-redirect threat class). Source: Lido WithdrawalQueue `requestWithdrawals` docs.
// bucket: protocol/lido
// tags: bucket:protocol cat:lido static venue:lido action-family:recipient-not-self
//
// Pure-static. `owner` is a required field (VOCAB §4 liquid_staking
// RequestWithdrawal); compare to `meta.submitter`.
@id("lido-request-withdrawal-owner-not-self")
@severity("warn")
@reason("This withdrawal request credits the unstETH/claimable ETH to another address — confirm the owner before signing")
forbid(principal, action == LiquidStaking::Action::"RequestWithdrawal", resource)
when {
  context.owner != context.meta.submitter
};
