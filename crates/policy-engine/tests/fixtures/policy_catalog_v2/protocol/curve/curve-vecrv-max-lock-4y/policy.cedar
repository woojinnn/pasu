// blocks: creating a veCRV lock whose unlock time is near the 4-year maximum, committing CRV illiquid for the longest possible term
// why:    a near-max (4-year) veCRV lock maximizes boost but also maximizes irreversibility — CRV is illiquid for the full term with no early exit; surface long locks for review. source: Curve DAO VotingEscrow docs (create_lock, MAXTIME = 4 years).
// bucket: protocol/curve
// tags: bucket:protocol cat:curve enrichment method:clock.now venue:curve action-family:irreversible-lock
// methods: clock.now -> ../../../_methods/clock.now.md
// dormant: until clock.now is registered in method-catalog.json
// NOTE: VOCAB §4 gives Lock `unlockTime:String` (U256 hex). Cedar cannot magnitude-compare
//       a hex String, so the lock-duration window (`unlockTime > nowTs + ~4y`) is NOT
//       expressible until the host projects a Long sibling for `unlockTime`. Until then this
//       rule presence-guards `nowTs` only and stays inert (no Long unlockTime to test against).
@id("curve-vecrv-max-lock-4y")
@severity("warn")
@reason("This veCRV lock runs near the 4-year maximum — confirm the long, irreversible lock term")
forbid(principal, action == Staking::Action::"Lock", resource)
when {
  context has custom
  && context.custom has nowTs
};
