// blocks: a transaction on a chain not in the user's allowed-chain set
// why:    chain-allowlisting (Ledger Academy network-hygiene guidance) confines activity to networks the user intends to use; a request on an unexpected chain (wrong-network phishing, look-alike RPC) is blocked
// bucket: wallet/chain-allowlist
// tags: bucket:wallet cat:chain-allowlist static action-family:chain-allowlist
// Binds the Core::Unknown catch-all; `chain` is the CAIP-2 chain id.
// Allowed-chain set is user-configured (CAIP-2 ids; placeholders below). Pure-static.
@id("chain-allowlist")
@severity("deny")
@reason("Transaction is on a chain outside your allowlist — blocked")
forbid(principal, action == Core::Action::"Unknown", resource)
unless {
  ["eip155:1", "eip155:8453", "eip155:42161"].contains(context.chain)
};
