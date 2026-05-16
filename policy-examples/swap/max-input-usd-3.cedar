@id("user/max-input-usd-3")
@severity("deny")
@reason("Total input value is $3 or more")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context has totalInputUsd &&
  context.totalInputUsd.value.greaterThanOrEqual(decimal("3.0000"))
};
