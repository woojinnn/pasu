// blocks: silently delegating all restaked shares to an EigenLayer operator who then holds slashing authority over your stake
// why:    restaking delegation hands an operator the power to get your stake slashed; unreviewed delegateTo is a stake-custody / slashing-authority risk class
// bucket: protocol/eigenlayer  (precedence-won)
// tags: bucket:protocol cat:eigenlayer static
// Catalog G2 — confirm before delegating all restaked shares to an operator
// (EigenLayer `delegateTo`). The operator gains slashing authority over your stake.
// Pure-static.
@id("restaking-delegate-confirm")
@severity("warn")
@reason("Delegating your restaked shares to an operator — confirm the operator (they gain slashing authority)")
forbid(principal, action == Restaking::Action::"DelegateTo", resource);
