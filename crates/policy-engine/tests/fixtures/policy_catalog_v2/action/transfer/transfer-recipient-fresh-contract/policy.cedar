// blocks: an ERC-20 transfer to a freshly-deployed contract (first seen within the last week) that may be a just-deployed drainer / collector.
// why:    fresh-drainer collector — wallet-drainer infrastructure rotates collector contracts; a recipient that is a contract first seen days ago is a strong drainer signal versus an established counterparty. Source: plan §2A transfer / MetaMask alerts.
// bucket: action/transfer
// tags: bucket:action cat:transfer enrichment method:address.activity action-family:transfer
// methods: address.activity -> ../../../_methods/address.activity.md
// Catalog (action/transfer) — warn when the recipient is a contract first observed
// within the last 7 days (604800 s). `address.activity` enriches `isContract` and
// (optionally) `firstSeenTs`; both are `has`-guarded. `meta.submittedAt` is the
// signing clock used as the age baseline.
// ENRICHMENT — dormant until the dispatcher serves `address.activity`.
// dormant: until address.activity is registered in method-catalog.json
@id("transfer-recipient-fresh-contract")
@severity("warn")
@reason("Recipient is a recently-deployed contract — possible freshly-created drainer; verify before sending")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has isContract
  && context.custom.isContract == true
  && context.custom has firstSeenTs
  && context.custom.firstSeenTs > (context.meta.submittedAt - 604800)
};
