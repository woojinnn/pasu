// Policy: only allow swaps whose `protocolId` is on a small allowlist.
// All other protocols (1inch, CowSwap, Curve, Balancer, …) → deny.
//
// Useful for users who want to scope swap exposure to vetted DEX surfaces.
// The allowlist is hand-edited for now; v0.5+ will accept template parameters
// so users can configure it via UI.

@id("user/uniswap-only-allowlist")
@severity("deny")
@reason("Swap protocol is not on the allowlist (uniswap-v2 / uniswap-v3 only)")
forbid (
    principal,
    action == Action::"swap",
    resource
)
unless {
    context has "protocolId" &&
    ["uniswap-v2", "uniswap-v3"].contains(context.protocolId)
};
