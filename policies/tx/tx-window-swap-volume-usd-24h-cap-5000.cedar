@id("user/tx-window-swap-volume-usd-24h-cap-5000")
@severity("deny")
@reason("Cumulative 24h swap USD volume exceeds the configured cap")
forbid (principal, action == Action::"send_tx", resource)
when {
  context has "windowStats"
  && context.windowStats has "swap_volume_usd_24h"
  && context.windowStats.swap_volume_usd_24h.greaterThan(decimal("5000.00"))
};
