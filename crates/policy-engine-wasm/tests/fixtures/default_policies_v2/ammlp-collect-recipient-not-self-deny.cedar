// scopeball default v2 policy — explorer id: AMMLP-1 (collect_fees output
// redirected to an address other than the signer).
//
// The sibling of the remove_liquidity recipient-redirect: harvesting accrued LP
// fees with `recipient` flipped to the attacker. `recipient` is a base,
// non-optional `String` field of `Amm::CollectFeesContext`, and `principal.address`
// is the evaluated wallet's address, so this compares directly with NO
// enrichment (sv = COV). Both sides are lowercase hex — exact equality holds.
@id("ammlp-collect-recipient-not-self-deny")
@severity("deny")
@reason("This fee collection sends the funds to an address that is not your wallet")
forbid(principal, action == Amm::Action::"CollectFees", resource)
when { context.recipient != principal.address };
