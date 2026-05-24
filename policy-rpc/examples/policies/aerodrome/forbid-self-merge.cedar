@id("default/aerodrome/forbid-self-merge")
@severity("deny")
@reason("VotingEscrow.merge with from == to is a no-op that wastes gas")
forbid (
  principal,
  action == Action::"lock_manage",
  resource
) when {
  context.kind == "merge" &&
  context has toTokenId &&
  context.fromTokenId == context.toTokenId
};
