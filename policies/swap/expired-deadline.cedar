@id("user/expired-deadline")
@severity("deny")
@reason("Swap deadline has already expired or is invalid")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context has validityDeltaSec &&
  context.validityDeltaSec <= 0
};
