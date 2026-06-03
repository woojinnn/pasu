// blocks: delegating (staking) HYPE to a Hyperliquid validator without confirming the validator and amount — undelegations are excluded.
// why:    staking-delegation authority — tokenDelegate locks HYPE under a chosen validator; a maliciously injected delegate sends stake to an attacker-favoured validator. Confirm the new delegation; undelegate (isUndelegate == true) is a withdrawal-direction action and is not gated here.
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-staking venue:hyperliquid
//
// Catalog — confirm before delegating HYPE to a Hyperliquid validator (hl docs).
// A staking-delegation action; gate only the delegate direction. Pure-static.
@id("hl-token-delegate-confirm")
@severity("warn")
@reason("Delegating HYPE to a Hyperliquid validator — confirm the validator and amount before the request is sent")
forbid(principal, action == HyperliquidCore::Action::"HlTokenDelegate", resource)
when { context.isUndelegate == false };
