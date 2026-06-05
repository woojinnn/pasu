// blocks: a full-balance collateral withdrawal (amount == uint256-max sentinel), which empties the supplied position and can drop any open borrow into liquidation territory.
// why:    Aave-style `withdraw(amount == type(uint256).max)` encodes "withdraw maximum"; pulling all collateral while debt is open removes the safety buffer and risks immediate liquidation. Source: Aave V3 Pool.withdraw docs (uint256.max = full withdraw).
// bucket: action/lending
// tags: bucket:action cat:lending static action-family:full-withdraw
// Pure-static: `amount` is U256 hex String; only the exact uint256-max sentinel
// is a valid equality compare (no numeric/range compares on hex strings).
@id("lending-collateral-withdraw-full")
@severity("warn")
@reason("This withdraws your entire supplied balance — confirm; if you have open debt this can trigger liquidation")
forbid(principal, action == Lending::Action::"Withdraw", resource)
when {
  context.amount == "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
};
