// blocks: creating a Curve veCRV vote-escrow lock without confirmation — the lock is non-reversible until its unlock time
// why:    Curve `VotingEscrow.create_lock` time-locks CRV with NO early-exit; tokens are illiquid until `unlockTime` (up to 4 years). A dApp prompting a lock commits funds irreversibly. source: Curve DAO VotingEscrow docs (create_lock).
// bucket: protocol/curve
// tags: bucket:protocol cat:curve static venue:curve action-family:irreversible-lock
@id("curve-vecrv-lock-non-reversible")
@severity("warn")
@reason("Creating a non-reversible veCRV lock — CRV is illiquid until the unlock time")
forbid(principal, action == Staking::Action::"Lock", resource)
when {
  context.venue.name == "curve_voting_escrow"
};
