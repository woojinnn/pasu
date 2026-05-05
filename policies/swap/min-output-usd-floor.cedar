// Policy: deny swaps whose minimum output USD value is below an absolute floor.
//
// Even with a non-zero `minOutputAmount.raw`, a swap can still leave the
// recipient with a tiny dollar value (e.g., 1 wei of WETH ≈ negligible).
// This policy enforces an absolute USD floor independent of token denomination.
//
// Skipped silently when oracle data is missing or stale (>60 s) — see
// the `max-swap-usd-100` companion for the input-side fail-closed pattern.

@id("user/min-output-usd-floor")
@severity("deny")
@reason("Minimum output USD value below floor (10.00) — high MEV / dust-out risk")
forbid (
    principal,
    action == Action::"swap",
    resource
)
when {
    context has "minOutputAmount" &&
    context.minOutputAmount has "usd" &&
    context.minOutputAmount.usd.staleSec <= 60 &&
    context.minOutputAmount.usd.value.lessThan(decimal("10.00"))
};
