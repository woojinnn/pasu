// blocks: an ERC-20 approve granting allowance to an EOA (externally-owned account) rather than a contract.
// why:    EOA-spender pull — a legitimate dApp spender is almost always a contract; approving an EOA usually means a phisher tricked the user into granting a person-controlled key direct pull rights over the token (revoke.cash approvals guidance).
// bucket: action/approval
// tags: bucket:action cat:approval enrichment method:address.activity action-family:approval
// methods: address.activity -> ../../../_methods/address.activity.md
// dormant: until address.activity is registered in method-catalog.json
//
// `address.activity` enriches whether the spender is a contract. Warn when the
// spender is an EOA (isContract == false). ENRICHMENT — dormant until served.
@id("approve-spender-is-eoa")
@severity("warn")
@reason("Spender is a regular wallet (EOA), not a contract — legitimate dApps use contracts; verify before approving")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has isContract
  && context.custom.isContract == false
};
