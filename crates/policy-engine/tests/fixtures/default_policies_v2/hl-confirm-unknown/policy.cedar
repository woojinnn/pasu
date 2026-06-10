@id("hl-confirm-unknown")
@severity("warn")
@reason("Unrecognized Hyperliquid action — confirm before the request is sent")
forbid(
  principal,
  action == Core::Action::"Unknown",
  resource
);
