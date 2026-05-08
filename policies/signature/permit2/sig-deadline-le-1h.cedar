// Policy: block Permit2 signatures whose signature deadline is more than 1h away.

@id("user/signature/permit2/sig-deadline-le-1h")
@severity("deny")
@reason("Permit2 signature deadline exceeds one hour")
forbid (
  principal is Wallet,
  action == Action::"signature.permit2",
  resource is Protocol
)
when {
  context.sigDeadlineDeltaSec > 3600
};
