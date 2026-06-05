// blocks: a transaction on a chain the user has flagged to avoid (warn-tier)
// why:    chain-denylisting (Ledger Academy network-hygiene guidance) warns when a request targets a network the user distrusts (high-fee, deprecated, or scam-prone testnet/sidechain) so they can reconsider before signing
// bucket: wallet/chain-denylist
// tags: bucket:wallet cat:chain-denylist static action-family:chain-denylist
// Binds the Core::Unknown catch-all; `chain` is the CAIP-2 chain id.
// Denied-chain set is user-configured (CAIP-2 ids; placeholders below). Pure-static.
@id("chain-denylist")
@severity("warn")
@reason("Transaction is on a chain you chose to avoid — confirm the network")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  ["eip155:56", "eip155:250"].contains(context.chain)
};
