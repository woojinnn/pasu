// Policy: block signature approvals whose approved value exceeds 100 USD.

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

@id("user/signature/eip2612/max-usd-100")
@severity("deny")
@reason("EIP-2612 approved USD value exceeds 100")
forbid (
  principal is Wallet,
  action == Action::"signature.eip2612",
  resource is Protocol
)
when {
  context has totalApprovedUsd
  && context.totalApprovedUsd.staleSec <= 60
  && context.totalApprovedUsd.value.greaterThan(decimal("100.00"))
};
