// blocks: an unconfirmed Aave borrow-rate-mode swap (variable <-> stable) on your existing debt
// why:    rate-profile-change threat class — swapping rate mode re-prices your whole borrow position (variable vs stable APR), changing future debt growth; the signer should consciously confirm the new mode. Source: Aave V3 Pool.swapBorrowRateMode docs.
// bucket: protocol/aave
// tags: bucket:protocol cat:aave static venue:aave_v3 action-family:swap-rate-mode
// Pure-static confirm: any rate-mode swap warns. newMode is the target mode
// (variable|stable); we confirm the action itself rather than a specific value.
@id("aave-swap-rate-mode-confirm")
@severity("warn")
@reason("Swapping your Aave borrow rate mode re-prices your debt (variable <-> stable) — confirm the change")
forbid(principal, action == Lending::Action::"SwapRateMode", resource);
