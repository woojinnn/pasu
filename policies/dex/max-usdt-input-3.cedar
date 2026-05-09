// Test policy: deny ANY Dex action whose total input value exceeds 3 USD.
//
// Looser than the original "USDT-only" version on purpose — for the manual
// test we want to know whether `totalInputUsd` is being populated at all.
// If even a 6 USDT swap doesn't trip this rule, the failure is upstream
// (oracle call missing, or the request is being classified as `other`
// instead of `dex`). If it DOES trip, we can re-tighten with the
// inputTokens.contains check.

@id("user/max-usdt-input-3")
@severity("deny")
@reason("Cannot swap more than 3 USD of input value")
forbid (
  principal is Wallet,
  action == Action::"dex",
  resource is Protocol
)
when {
  context has totalInputUsd
  && context.totalInputUsd.value.greaterThan(decimal("3.00"))
};
