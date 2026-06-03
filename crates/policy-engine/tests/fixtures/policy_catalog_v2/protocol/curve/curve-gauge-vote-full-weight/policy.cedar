// blocks: allocating your ENTIRE veCRV vote weight (10000 bp) to a single gauge in one vote
// why:    Curve `GaugeController.vote_for_gauge_weights` with `_user_weight == 10000` commits 100% of voting power to one gauge AND locks that weight for ~10 days (vote-cooldown), concentrating governance influence with no room to hedge. source: Curve DAO GaugeController docs (vote_for_gauge_weights, 10-day vote lock).
// bucket: protocol/curve
// tags: bucket:protocol cat:curve static venue:curve action-family:governance-vote
@id("curve-gauge-vote-full-weight")
@severity("warn")
@reason("Allocating 100% of your veCRV weight to one gauge — confirm this full-weight, ~10-day-locked vote")
forbid(principal, action == Staking::Action::"VoteForGauge", resource)
when {
  context.weightBp == "0x2710"
};
