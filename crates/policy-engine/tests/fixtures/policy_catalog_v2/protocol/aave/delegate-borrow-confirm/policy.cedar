// blocks: a silent Aave credit-delegation grant that lets another address borrow against your collateral
// why:    delegated-authorization / collateral-drain threat class — delegateCredit is a high-trust grant the signer must consciously confirm
// bucket: protocol/aave  (precedence-won)
// tags: bucket:protocol cat:aave static
// Catalog E5 — confirm Aave credit delegation.
// `delegateCredit` lets another address borrow against YOUR collateral — a
// high-trust grant. Confirm the delegatee and amount. Pure-static.
@id("delegate-borrow-confirm")
@severity("warn")
@reason("Delegating credit lets another address borrow against your collateral — confirm the delegatee")
forbid(principal, action == Lending::Action::"DelegateBorrow", resource);
