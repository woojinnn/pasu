// blocks: silently approving a Hyperliquid agent (API) wallet that can then place orders on your behalf without further prompts.
// why:    delegated-trading authority — an approved agent key signs orders unattended (the delegation behind "Enable trading"); confirming the agent address guards against approving an attacker-controlled key.
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static
//
// Catalog G5 — confirm before approving a Hyperliquid agent (API) wallet.
// An approved agent can place orders on your behalf without further prompts —
// the delegation behind "Enable trading". Pure-static.
@id("hl-approve-agent-confirm")
@severity("warn")
@reason("Approving a Hyperliquid agent wallet — it can trade on your behalf; confirm the agent address")
forbid(principal, action == HyperliquidCore::Action::"HlApproveAgent", resource);
