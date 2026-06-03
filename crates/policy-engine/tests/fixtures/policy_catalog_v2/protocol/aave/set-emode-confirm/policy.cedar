// blocks: silently enabling an Aave e-mode category that raises LTV and shifts your liquidation thresholds
// why:    lending-risk-profile-change threat class — e-mode alters borrow power and liquidation profile, so the signer must confirm the category
// bucket: protocol/aave  (precedence-won)
// tags: bucket:protocol cat:aave static
// Catalog E4 — confirm when enabling an Aave e-mode category.
// e-mode raises LTV for a correlated-asset category but changes your liquidation
// profile; `categoryId == 0` disables (allowed), any other id enables (warn).
// Pure-static (`categoryId` is Long).
@id("set-emode-confirm")
@severity("warn")
@reason("Enabling e-mode changes your borrow power and liquidation thresholds — confirm the category")
forbid(principal, action == Lending::Action::"SetEMode", resource)
when { context.categoryId != 0 };
