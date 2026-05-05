// Policy: warn when a swap's minimum output amount is exactly zero.
//
// `minOutputAmount.raw == "0"` means the caller accepts ANY positive output
// (including a single wei). This is almost always a misconfiguration or an
// MEV-vulnerable pattern: a sandwich attacker can push the actual output to
// near-zero and the swap still succeeds.
//
// We emit a warning rather than a deny so that legitimate exact-out swaps
// (where amountOutMin / minOutputAmount can rationally be 0 because the
// caller specifies amountOut and ceiling-bounds amountIn) aren't blocked.

@id("user/no-zero-min-output")
@severity("warn")
@reason("Swap minimum output is zero — the swap will accept any positive output, including 1 wei (MEV-vulnerable)")
forbid (
    principal,
    action == Action::"swap",
    resource
)
when {
    context has "minOutputAmount" &&
    context.minOutputAmount.raw == "0"
};
