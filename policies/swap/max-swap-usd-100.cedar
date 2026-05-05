// Policy: a swap whose USD-denominated input value exceeds 100 USD is denied.
//
// "100 USDT 이상 swap할 수 없다" is interpreted as "more than 100 USD-equivalent",
// because USDT is a stablecoin proxy for USD. With the v0.1 mock oracle that
// pegs USDT at 1.00 USD this is exact for USDT inputs and approximate for
// other tokens.
//
// `staleSec <= 60` guards against accepting an extremely stale oracle reading.

@id("user/max-swap-usd-100")
@severity("deny")
@reason("USD value of swap input exceeds 100")
forbid (
    principal,
    action == Action::"swap",
    resource
)
when {
    context has "inputAmount" &&
    context.inputAmount has "usd" &&
    context.inputAmount.usd.staleSec <= 60 &&
    context.inputAmount.usd.value.greaterThan(decimal("100.00"))
};
