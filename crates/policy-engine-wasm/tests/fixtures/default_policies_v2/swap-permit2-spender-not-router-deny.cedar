// scopeball default v2 policy — explorer id: UNI-09 (Permit2 spender is not the
// real router → phishing).
//
// Permit2 phishing captures a signature whose `spender` is the attacker, then
// drains the token wholesale (the PEPE $1.39M case). A legitimate swap's Permit2
// spender must be the router that actually executes the swap. The Permit2
// spender and the executing router (route aggregator.router / venue) are NOT both
// on the swap context, and matching them against a known-router set is external,
// so the method `external.permit2_spender_is_router` does the reconciliation and
// emits the Bool `permitSpenderIsRouter`. `custom` and the call are optional, so
// the deny only fires once the check has run AND returned false (fail-open).
@id("swap-permit2-spender-not-router-deny")
@severity("deny")
@reason("The Permit2 approval spender is not the router that executes this swap — a phishing signature")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has permitSpenderIsRouter
  && context.custom.permitSpenderIsRouter == false
};
