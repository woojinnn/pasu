// blocks: sending a spot token off the Hyperliquid account to a destination without an explicit confirm.
// why:    fund-movement exfiltration — spotSend moves a spot balance off the account to an arbitrary destination; an injected send is direct, irreversible loss, so confirm the destination and token unconditionally. (hl docs — spotSend)
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-transfer
//
// Catalog — confirm a Hyperliquid spotSend (spot token transfer off the account).
// spotSend now lowers to the generic Token::Erc20Transfer (recipient = destination,
// token = HL spot token); the manifest's `hl_spot_send` trigger still scopes this
// policy to HL spot sends, so it stays an unconditional pure-static confirm.
@id("hl-spot-send-confirm")
@severity("warn")
@reason("Sending a spot token off Hyperliquid — confirm the destination and token before the request is sent")
forbid(principal, action == Token::Action::"Erc20Transfer", resource);
