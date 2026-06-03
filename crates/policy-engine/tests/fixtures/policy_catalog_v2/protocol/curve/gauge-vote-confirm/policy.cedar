// blocks: a veCRV gauge-weight vote reallocating your vote-escrow weight to the wrong gauge/weight without explicit confirmation
// why:    governance vote-weight allocation — a dApp can silently steer your veCRV voting power; surface the gauge+weight for review (DeFi governance-manipulation threat class)
// bucket: protocol/curve  (precedence-won)
// tags: bucket:protocol cat:curve static
// Catalog G4 — confirm a Curve gauge-weight vote (veCRV `vote_for_gauge_weights`).
// Reallocates your vote-escrow weight; confirm the gauge and weight. Pure-static.
@id("gauge-vote-confirm")
@severity("warn")
@reason("Allocating veCRV gauge weight — confirm the gauge and weight before voting")
forbid(principal, action == Staking::Action::"VoteForGauge", resource);
