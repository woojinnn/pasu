@id("user/expired-deadline")
@severity("deny")
@reason("Swap deadline has already expired or is invalid")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context has custom &&
  context.custom has validityDeltaSec &&
  context.custom.validityDeltaSec <= 0
};
