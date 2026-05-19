@id("user/swap-short-deadline")
@severity("warn")
@reason("Swap deadline 이 60초 미만 — sandwich attack 또는 atomic execution 강제 가능성")
forbid (
  principal,
  action == Action::"swap",
  resource
) when {
  context has validityDeltaSec &&
  context.validityDeltaSec > 0 &&
  context.validityDeltaSec < 60
};
