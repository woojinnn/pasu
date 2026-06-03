// blocks: a swap whose output recipient is not on the user's destination allowlist (deny-tier)
// why:    routers let the swap-output recipient differ from the sender; a hard allowlist (Coinbase CDP policy model) prevents output from being silently diverted to an attacker-controlled address
// bucket: wallet/recipient-allowlist
// tags: bucket:wallet cat:recipient-allowlist static action-family:recipient-allowlist
// User-configured allowlist (lowercase 0x; placeholders below). Pure-static.
@id("swap-output-recipient-allowlist")
@severity("deny")
@reason("Swap output recipient is not on your allowlist — swap blocked")
forbid(principal, action == Amm::Action::"Swap", resource)
unless {
  ["0x000000000000000000000000000000000000dead", "0x1111111111111111111111111111111111111111"].contains(context.recipient)
};
