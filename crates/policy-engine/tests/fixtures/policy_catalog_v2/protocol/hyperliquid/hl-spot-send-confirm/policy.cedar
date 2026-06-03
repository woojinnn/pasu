// blocks: sending a spot token off the Hyperliquid account to a destination without an explicit confirm.
// why:    fund-movement exfiltration — spotSend moves a spot balance off the account to an arbitrary destination; an injected send is direct, irreversible loss, so confirm the destination and token unconditionally. (hl docs — spotSend)
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-transfer
//
// Catalog — confirm a Hyperliquid spotSend (spot token transfer off the account).
// HL amounts are decimal strings (magnitude not comparable in Cedar), so this is
// an unconditional pure-static confirm on the action.
@id("hl-spot-send-confirm")
@severity("warn")
@reason("Sending a spot token off Hyperliquid — confirm the destination and token before the request is sent")
forbid(principal, action == HyperliquidCore::Action::"HlSpotSend", resource);
