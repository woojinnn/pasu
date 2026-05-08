// Policy: block Permit2 signatures whose total approved value exceeds 100 USD.

@id("user/signature/permit2/max-usd-100")
@severity("deny")
@reason("Permit2 approved USD value exceeds 100")
forbid (
  principal is Wallet,
  action == Action::"signature.permit2",
  resource is Protocol
)
when {
  context has totalApprovedUsd
  && context.totalApprovedUsd.staleSec <= 60
  && context.totalApprovedUsd.value.greaterThan(decimal("100.00"))
};
