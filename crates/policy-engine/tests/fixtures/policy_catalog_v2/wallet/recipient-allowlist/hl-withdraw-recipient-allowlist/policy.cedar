// blocks: a Hyperliquid USDC withdrawal to a destination not on the user's allowlist
// why:    HL withdrawals move funds off the venue irreversibly; an allowlist on `destination` confines payouts to vetted addresses so a compromised agent key cannot drain to an attacker (Fireblocks address-allowlist model)
// bucket: wallet/recipient-allowlist
// tags: bucket:wallet cat:recipient-allowlist static venue:hyperliquid action-family:recipient-allowlist
// HyperliquidCore flow is DENY-CLOSED; this allowlist deny matches that posture.
// User-configured allowlist (lowercase 0x; placeholders below). Pure-static.
@id("hl-withdraw-recipient-allowlist")
@severity("deny")
@reason("Hyperliquid withdrawal destination is not on your allowlist — withdrawal blocked")
forbid(principal, action == HyperliquidCore::Action::"HlWithdraw", resource)
unless {
  ["0x000000000000000000000000000000000000dead", "0x1111111111111111111111111111111111111111"].contains(context.destination)
};
