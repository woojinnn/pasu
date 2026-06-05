// blocks: a liquid-staking ETH deposit (e.g. Lido `submit`) auto-signing without the user confirming the staking venue and amount.
// why:    liquid-staking deposit confirmation — native ETH is handed to a staking contract for a liquid-staking token; threat class = unintended custody handover / wrong-venue deposit.
// bucket: protocol/lido  (precedence-won)
// tags: bucket:protocol cat:lido static
// Catalog I5 — confirm a liquid-staking deposit (e.g. Lido `submit`).
// Stakes native ETH for a liquid-staking token; confirm the venue and amount.
// Pure-static.
@id("liquid-stake-confirm")
@severity("warn")
@reason("Staking ETH for a liquid-staking token — confirm the venue and amount")
forbid(principal, action == LiquidStaking::Action::"Stake", resource);
