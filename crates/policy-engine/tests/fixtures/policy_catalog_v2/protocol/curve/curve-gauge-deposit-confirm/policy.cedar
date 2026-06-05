// blocks: staking Curve LP tokens into a liquidity gauge without confirmation
// why:    Curve `LiquidityGauge.deposit` moves your LP tokens into the gauge contract (custody transfer to start earning CRV); confirm the gauge and amount before depositing. source: Curve DAO LiquidityGauge docs (deposit).
// bucket: protocol/curve
// tags: bucket:protocol cat:curve static venue:curve action-family:stake-deposit
@id("curve-gauge-deposit-confirm")
@severity("warn")
@reason("Staking LP tokens into a Curve gauge — confirm the gauge and deposit amount")
forbid(principal, action == Staking::Action::"GaugeDeposit", resource);
