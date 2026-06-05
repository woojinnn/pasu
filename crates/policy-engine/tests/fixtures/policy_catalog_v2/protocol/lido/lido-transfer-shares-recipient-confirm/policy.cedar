// blocks: a Lido `transferShares` that moves stETH shares to an address other than the signer.
// why:    share-transfer redirection — `transferShares(recipient, shares)` moves pooled-ETH shares (≈ rebasing stETH balance) to `recipient`; if `recipient` is not the signer this hands custody of staked value to a third party (fund-redirect threat class). Source: Lido stETH `transferShares` docs.
// bucket: protocol/lido
// tags: bucket:protocol cat:lido static venue:lido action-family:recipient-not-self
//
// Pure-static. `recipient` is a required field (VOCAB §4 liquid_staking
// TransferShares); compare to `meta.submitter`.
@id("lido-transfer-shares-recipient-confirm")
@severity("warn")
@reason("This share transfer sends stETH shares to another address — confirm the recipient before signing")
forbid(principal, action == LiquidStaking::Action::"TransferShares", resource)
when {
  context.recipient != context.meta.submitter
};
