// blocks: bridging funds off Hyperliquid to an EVM recipient carrying arbitrary, attacker-controllable calldata without an explicit confirm.
// why:    fund-exfiltration via arbitrary calldata — sendToEvmWithData is the highest-risk HL fund movement, combining a bridge-out with caller-supplied calldata that can re-route or trap the funds; confirm the recipient unconditionally.
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static
//
// Catalog F6 — confirm a Hyperliquid sendToEvmWithData (bridge-out + arbitrary calldata).
// The highest-risk HL fund movement: bridges a token to an EVM recipient WITH
// attacker-controllable calldata. Confirm unconditionally. Pure-static.
@id("hl-send-to-evm-with-data-confirm")
@severity("warn")
@reason("Bridging funds to an EVM address with arbitrary calldata — confirm the recipient before sending")
forbid(principal, action == HyperliquidCore::Action::"HlSendToEvmWithData", resource);
