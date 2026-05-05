// Policy: deny swaps whose protocol fee tier exceeds 100 basis points (1.0%).
//
// `feeBips` is what the adapter reports for the swap's protocol-level fee:
//   - Uniswap V2 LP fee: hardcoded 30 bps (always allowed by this policy).
//   - Uniswap V3 single-pool: the V3 fee tier (100 / 500 / 3000 / 10000).
//   - Uniswap V3 multi-hop: the average across hops.
// The policy guards with `has "feeBips"` so it skips silently for adapters
// that don't disclose a fee — see `require-fee-disclosure` for a stricter
// companion that warns on missing fee data.

@id("user/max-swap-fee-bps-100")
@severity("deny")
@reason("Swap fee tier exceeds 100 bps (1.0%)")
forbid (
    principal,
    action == Action::"swap",
    resource
)
when {
    context has "feeBips" &&
    context.feeBips > 100
};
