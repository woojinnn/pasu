// scopeball default v2 policy — explorer id: LP-03 (cumulative commitments
// across multiple sales exceed a personal cap).
//
// Launchpads stoke FOMO into over-committing across many sales at once. The
// defence folds every currently-held `LaunchpadAllocation.paid` plus THIS
// commit into one USD figure and warns when it tops the user's cap.
//
// sv = COV for the live-holdings view (durable state has the open allocations);
// the time-window history sum (already withdrawn/completed sales) is GAP —
// there is no history surface — and is out of scope here.
//
// Cedar cannot sum a `Vec<(TokenRef, U256)>` across positions nor multiply by
// price, so this is a FOLD method (`launchpad.cumulative_committed_usd`,
// sim-server, over `positions(launchpad_allocation).paid` × `price_value`,
// adding the in-flight commit). It emits `context.custom.cumulativePaidUsd`
// (decimal). The call is `optional`: until the method ships its absence
// fail-opens (`context.custom has cumulativePaidUsd` is false -> no-op). The
// threshold below is a placeholder for the user's `launchpad_total_cap` (USD).
@id("lp-commit-cumulative-cap-warn")
@severity("warn")
@reason("Your total open launchpad commitments exceed your personal cap")
forbid(principal, action == Launchpad::Action::"Commit", resource)
when {
  context has custom
  && context.custom has cumulativePaidUsd
  && context.custom.cumulativePaidUsd.greaterThan(decimal("10000.0"))
};
