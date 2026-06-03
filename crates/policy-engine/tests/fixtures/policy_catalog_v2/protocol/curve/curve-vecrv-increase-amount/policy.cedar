// blocks: adding more CRV to an existing veCRV lock without confirmation — the added amount inherits the lock's illiquidity
// why:    Curve `VotingEscrow.increase_amount` deposits additional CRV into the time-lock; the added tokens cannot be withdrawn until the existing unlock time. Confirm the top-up. source: Curve DAO VotingEscrow docs (increase_amount).
// bucket: protocol/curve
// tags: bucket:protocol cat:curve static venue:curve action-family:irreversible-lock
@id("curve-vecrv-increase-amount")
@severity("warn")
@reason("Adding CRV to your veCRV lock — the top-up stays locked until the existing unlock time")
forbid(principal, action == Staking::Action::"IncreaseLockAmount", resource);
