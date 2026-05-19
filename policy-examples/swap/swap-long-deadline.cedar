@id("user/swap-long-deadline")
@severity("warn")
@reason("Swap deadline 이 1시간 초과 — rebroadcast 위험 (MEV / front-run)")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context has validityDeltaSec &&
  context.validityDeltaSec > 3600
};
