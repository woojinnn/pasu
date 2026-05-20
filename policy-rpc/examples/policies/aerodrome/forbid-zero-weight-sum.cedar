@id("default/aerodrome/forbid-zero-weight-sum")
@severity("warn")
@reason("Gauge vote with zero weight sum is meaningless except for explicit reset")
forbid (
  principal,
  action == Action::"gauge_vote",
  resource
) when {
  context.kind != "reset" &&
  context.weightsSum == "0"
};
