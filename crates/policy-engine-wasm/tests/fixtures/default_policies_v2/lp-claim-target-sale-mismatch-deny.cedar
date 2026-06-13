// dambi default v2 policy — explorer id: LP-01 (claim from a contract that
// is NOT the sale I originally subscribed to).
//
// Token-distribution (claim) is when fake-claim sites are most active: a
// look-alike "airdrop/claim" contract drains the user. The defence compares the
// claim target against the sale the wallet actually committed to, recorded in
// durable state as `LaunchpadAllocation{platform, sale_id}`.
//
// sv = PART per the explorer: state holds (platform name, sale_id), but
// `platform` is a `ProtocolRef` (NAME only, no contract address), so resolving
// the legitimate claim-contract address needs an external registry
// (ProtocolRef -> address). The `launchpad.sale_match` method (sim-server,
// reads `positions(launchpad_allocation).sale_id`, with the registry resolve on
// the host side) folds that comparison into a single Bool
// `context.custom.targetMatchesSale`.
//
// The call is `optional`, so until the sim-server method + registry resolve
// ship, its absence fail-opens: `context.custom has targetMatchesSale` is false
// and the policy no-ops rather than SystemFail. Deny when the target does NOT
// match the recorded sale.
@id("lp-claim-target-sale-mismatch-deny")
@severity("deny")
@reason("This claim contract does not match the sale you originally subscribed to")
forbid(principal, action == Launchpad::Action::"ClaimAllocation", resource)
when {
  context has custom
  && context.custom has targetMatchesSale
  && context.custom.targetMatchesSale == false
};
