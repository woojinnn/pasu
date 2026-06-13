// dambi default v2 policy — H2 fund-egress redirect guard (Send Safety Set).
//
// A Uniswap router `sweepToken` / `unwrapWETH9` with an EXPLICIT `recipient`
// moves the router's OWN held balance — e.g. the output of a swap you just
// routed through it — to that address. Hidden as a leg inside a swap multicall,
// a `recipient != your wallet` sweep silently redirects the swap output to an
// attacker while the visible swap leg looks normal. The decoder marks such a
// router egress with `is_router_egress = true` on the lowered `Erc20Transfer`
// (a normal user `transfer` leaves the flag ABSENT), so this judges ONLY sweeps
// — ordinary sends are never touched, avoiding alarm fatigue.
//
// `recipient` and `principal.address` are lowercase hex on both sides
// (host-normalized), so exact String equality holds. `warn` (not `deny`): a
// sweep to a different address CAN be intentional (e.g. routing output to
// another wallet you own), so surface a one-beat warn the user can approve.
@id("sweep-recipient-not-self-warn")
@severity("warn")
@reason("A router sweep/unwrap sends its output to an address that is not your wallet — the swap proceeds may be redirected to someone else")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has is_router_egress &&
  context.is_router_egress &&
  context.recipient != principal.address
};
