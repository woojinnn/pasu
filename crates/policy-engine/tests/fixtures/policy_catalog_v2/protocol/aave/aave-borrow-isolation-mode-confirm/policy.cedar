// blocks: an unconfirmed borrow on Aave V3 where isolation-mode rules cap which assets you may borrow and against which single collateral
// why:    lending-risk-profile threat class — borrowing under Aave V3 isolation mode constrains debt to the isolated collateral and a per-asset ceiling, so the signer must consciously confirm the borrow venue. Source: Aave V3 isolation-mode docs.
// bucket: protocol/aave
// tags: bucket:protocol cat:aave static venue:aave_v3 could-be-compliance:true
// Pure-static: confirm any Aave V3 borrow (isolation-mode debt ceiling / single-
// collateral constraint applies). `venue.name` is the LendingVenue discriminator.
@id("aave-borrow-isolation-mode-confirm")
@severity("warn")
@reason("Borrowing on Aave V3 — isolation mode limits borrowable assets and ceilings; confirm the position")
forbid(principal, action == Lending::Action::"Borrow", resource)
when { context.venue.name == "aave_v3" };
