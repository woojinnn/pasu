// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// scopeball draft policy — Degen Self-Control Set / self-exclusion (gambling
// addiction harm-reduction). Signal: crypto-gambling-addiction-cant-stop.
//
// Demand (strong / 3 distinct sources): repeated failed attempts to stop, 12%
// of investors borrow to chase losses, and self-exclusion + access blocking is
// named as an effective intervention (addictioncenter, rightchoicerecovery,
// gamblingaddictionhotline).
//
// INTENT: once the user self-excludes a venue/dApp (e.g. a perp DEX or a
// degen swap router), every subsequent swap/sign routed to that venue is DENY-
// blocked. Lifting the exclusion is deliberately high-friction: a cooldown
// delay must elapse (lockedUntil) AND/OR a third-party approver must co-sign.
// The wallet must NOT be able to silently flip the block off mid-impulse.
//
// BLOCKED-BY-STATE: the load-bearing piece is the SelfExclusion state surface,
// which does NOT exist in policy-engine today (stateless, single-request).
// What is missing:
//   1. A persisted SelfExclusion FSM keyed by (owner, venue|domain) holding
//      { active: Bool, lockedUntil: unix_secs, pendingApprover?: Address }.
//   2. Re-enablement guard: turning active=false requires now >= lockedUntil
//      (cooldown delay) OR a co-signature from pendingApprover. Cedar evaluates
//      one request and cannot enforce a cross-request timer / approval handshake.
// Cedar below can only COMPARE a pre-computed `venueSelfExcluded` Bool projected
// by the state surface; it cannot itself guarantee the exclusion's integrity.
// Until the SelfExclusion state surface ships, the `order.venue_self_excluded`
// call is `optional` (fail-open): `context.custom has venueSelfExcluded` is
// false and this policy no-ops rather than SystemFail. It activates once state
// lands.
@id("self-exclusion-block-deny")
@severity("deny")
@reason("You self-excluded this venue. The block stays until the cooldown ends or your approver lifts it.")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has venueSelfExcluded
  && context.custom.venueSelfExcluded
};
