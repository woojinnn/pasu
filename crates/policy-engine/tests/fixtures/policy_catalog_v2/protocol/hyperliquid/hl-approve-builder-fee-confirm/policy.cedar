// blocks: approving a Hyperliquid builder fee without confirming the builder address and the max fee rate.
// why:    delegated-fee authority — approveBuilderFee lets a named builder skim up to maxFeeRate from your fills on every order routed through it; a maliciously injected approval grants an attacker-controlled builder an ongoing fee claim. Confirm the builder and rate unconditionally.
// bucket: protocol/hyperliquid  (precedence-won)
// tags: bucket:protocol cat:hyperliquid static action-family:hl-fee venue:hyperliquid
//
// Catalog — confirm before approving a Hyperliquid builder fee (hl docs).
// Grants a named builder an ongoing fee claim on your fills. Pure-static.
@id("hl-approve-builder-fee-confirm")
@severity("warn")
@reason("Approving a Hyperliquid builder fee — confirm the builder address and max fee rate")
forbid(principal, action == HyperliquidCore::Action::"HlApproveBuilderFee", resource);
