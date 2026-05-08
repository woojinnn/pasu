// Policy: block EIP-2612 permits whose deadline is more than 1h away.

@id("user/signature/eip2612/deadline-le-1h")
@severity("deny")
@reason("EIP-2612 deadline exceeds one hour")
forbid (
  principal is Wallet,
  action == Action::"signature.eip2612",
  resource is Protocol
)
when {
  context.deadlineDeltaSec > 3600
};
