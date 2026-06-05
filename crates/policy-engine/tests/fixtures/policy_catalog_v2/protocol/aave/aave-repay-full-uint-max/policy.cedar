// blocks: an Aave repay using the uint256-max sentinel, which pulls the FULL outstanding debt from your wallet
// why:    unbounded-spend threat class — Aave V3 repay(type(uint256).max) repays the entire current debt, so the actual token amount leaving your wallet is open-ended and only known at execution; the signer should confirm a full-debt repay. Source: Aave V3 Pool.repay docs (uint256 max = full debt).
// bucket: protocol/aave
// tags: bucket:protocol cat:aave static venue:aave_v3 action-family:repay
// Pure-static: amount is a raw U256 hex String; the only safe compare is the
// exact uint256-max sentinel (64 f's). Matches the "repay full debt" magic value.
@id("aave-repay-full-uint-max")
@severity("warn")
@reason("Repaying with the uint256-max sentinel clears your entire Aave debt — confirm the full-debt amount")
forbid(principal, action == Lending::Action::"Repay", resource)
when {
  context.amount == "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
};
