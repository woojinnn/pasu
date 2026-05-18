@id("user/max-input-usd-100")
@severity("deny")
@reason("Total input value exceeds limit")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context has custom &&
  context.custom has totalInputUsd &&
  context.custom.totalInputUsd.value.greaterThan(decimal("100.0000"))
};
