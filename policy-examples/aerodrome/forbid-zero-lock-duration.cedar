@id("default/aerodrome/forbid-zero-lock-duration")
@severity("deny")
@reason("VotingEscrow.createLock with zero duration locks funds without voting power")
forbid (
  principal,
  action == Action::"lock_create",
  resource
) when {
  context.lockDurationSec == "0"
};
