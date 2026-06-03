// blocks: silently moving all your restaked shares from your current EigenLayer operator to a new one (atomic undelegate+delegate to newOperator).
// why:    `redelegate` switches who holds slashing authority over your entire stake in one step; an unreviewed newOperator inherits full slashing power (stake-custody / slashing-authority transfer threat class). Source: EigenLayer DelegationManager docs.
// bucket: protocol/eigenlayer
// tags: bucket:protocol cat:eigenlayer static venue:eigenlayer action-family:restaking
//
// Pure-static — anchor an explicit confirm on every redelegation; the operator
// being switched to gains slashing authority over your restaked stake.
@id("eigenlayer-redelegate-confirm")
@severity("warn")
@reason("Re-delegating your restaked shares to a new operator — confirm the new operator (they gain slashing authority)")
forbid(principal, action == Restaking::Action::"Redelegate", resource);
