// blocks: an ERC-20 approve granting allowance to a freshly-deployed contract (deployed within the last week).
// why:    fresh-drainer pattern — newly-deployed spender contracts are a strong drainer signal (the contract is created just before the phishing campaign); MetaMask/Blockaid surface "brand-new contract" warnings on approvals.
// bucket: action/approval
// tags: bucket:action cat:approval enrichment method:address.activity action-family:approval
// methods: address.activity -> ../../../_methods/address.activity.md
// dormant: until address.activity is registered in method-catalog.json
//
// `address.activity` enriches the spender's contract-ness and first-seen time.
// Warn when the spender is a contract first observed < 7 days (604800s) ago.
// ENRICHMENT — dormant until the dispatcher serves the method.
@id("approve-spender-fresh-contract")
@severity("warn")
@reason("Spender is a brand-new contract (deployed within the last week) — verify this is the dApp you intend to approve")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has isContract
  && context.custom.isContract == true
  && context.custom has firstSeenTs
  && context.custom.firstSeenTs > context.meta.submittedAt - 604800
};
