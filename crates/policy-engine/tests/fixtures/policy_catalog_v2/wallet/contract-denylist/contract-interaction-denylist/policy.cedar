// blocks: any transaction whose target contract is on the user/operator denylist
// why:    a catch-all contract denylist (ThreatLocker block-policy) hard-blocks interaction with addresses flagged malicious/compromised even when no manifest decoded the call; `target` is the called contract
// bucket: wallet/contract-denylist
// tags: bucket:wallet cat:contract-denylist static action-family:contract-denylist could-be-compliance:true
// Binds the Core::Unknown catch-all (no manifest matched). Denied-contract set is
// user/operator-configured (lowercase 0x; placeholders below). Pure-static.
@id("contract-interaction-denylist")
@severity("deny")
@reason("Target contract is on your denylist of known-malicious addresses — interaction blocked")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  ["0x000000000000000000000000000000000000dead", "0x2222222222222222222222222222222222222222"].contains(context.target)
};
