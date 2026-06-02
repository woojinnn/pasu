// scopeball default v2 policy — explorer id: NFT-05 (unlimited / over-cap WETH
// approval granted to a bidding conduit; the NFT-bid instance of GEN-01).
//
// Bidding on NFTs requires pre-approving WETH to the marketplace conduit. An
// `approve(spender, U256::MAX)` (or an amount far over what a bid needs) turns
// that conduit into an open-ended spend channel. `amount` is a raw U256 hex
// String on `Erc20ApproveContext`, so "unlimited" CANNOT be tested in Cedar
// directly — it arrives as the enriched Bool `bidApprovalIsUnlimited`,
// projected by REUSING the existing `approval.unlimited_over_balance` policy_rpc
// (Aggregator type 1 — Direct State read of `approvals_erc20.is_unlimited` ×
// `token_holdings.balance_amount`). The companion `bidApprovalOverBalance`
// (decimal, approve-amount / available-balance) is also projected for the
// over-cap scoring; this slice keys the warn on the unlimited flag, leaving the
// personalized `bid_weth_cap` threshold to a later increment.
//
// `custom` is optional, so the guard `has`-checks the path before reading it;
// the call is `optional`, so its absence fail-opens until the fact host ships.
@id("nft-bid-weth-unlimited-warn")
@severity("warn")
@reason("this WETH approval is unlimited — a bid only needs the offer amount, so cap it")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has bidApprovalIsUnlimited
  && context.custom.bidApprovalIsUnlimited
};
