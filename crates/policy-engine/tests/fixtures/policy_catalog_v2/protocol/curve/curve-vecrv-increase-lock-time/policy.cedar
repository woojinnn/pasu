// blocks: extending an existing veCRV lock's unlock time without confirmation — a longer lock further delays withdrawal
// why:    Curve `VotingEscrow.increase_unlock_time` pushes the unlock further out (up to 4 years), extending the period CRV is illiquid; surface the action for review. source: Curve DAO VotingEscrow docs (increase_unlock_time).
// bucket: protocol/curve
// tags: bucket:protocol cat:curve static venue:curve action-family:irreversible-lock
@id("curve-vecrv-increase-lock-time")
@severity("warn")
@reason("Extending your veCRV lock time — CRV stays illiquid longer; confirm the extension")
forbid(principal, action == Staking::Action::"IncreaseLockTime", resource);
