@id("user/max-input-usd-100")
@severity("deny")
@reason("Total input value exceeds  limit")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  (context has rpcTotalInputUsd &&
   context.rpcTotalInputUsd.value.greaterThan(decimal("100.0000"))) ||
  (context has totalInputUsd &&
   context.totalInputUsd.value.greaterThan(decimal("100.0000")))
};
