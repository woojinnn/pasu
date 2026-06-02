// scopeball default v2 policy — explorer id: H3 / VALUE (personal allow/deny
// list for tokens / contracts).
//
// Personal-trading (TradFi pre-trade) family. Deny an AMM swap whose `recipient`
// (the address that will receive the bought tokens) is on the user's personal
// denylist. This is the canonical Set-Membership control (Aggregator type 5):
// the user keeps a local "never route to / never trade these" list.
//
// sv = COV today (per tradfi mapping §H3): no enrichment, no external feed. The
// denylist is LOCAL user config and — per the team decision recorded in the
// coverage doc — is kept as a Cedar `Set<String>` LITERAL for now (a future
// local handler→Bool projection is the personalized-supply increment, mirroring
// the GEN-01 allowlist literal). Because the set is a literal, this manifest
// declares NO `policy_rpc` and NO `custom_context` — like `swap-slippage-high-warn`.
//
// CHOICE: we denylist on `context.recipient` (a base `String` field of
// `Amm::SwapContext`, always present) rather than on `context.tokenOut`. The
// token identity lives at `context.tokenOut.key.address`, which is an OPTIONAL
// nested field (`address?`) and would require chained `has`-guards to read in
// Cedar; the `recipient` String is non-optional and tests cleanly. A token
// denylist is the symmetric variant once a `tokenOutAddress` projection (or the
// nested-`has` form) is wired — see the coverage doc's H3 note. Addresses are
// lowercase hex; the sample entries below are placeholders.
@id("values-recipient-denylist-deny")
@severity("deny")
@reason("This swap routes to an address on your personal denylist")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  ["0x000000000000000000000000000000000000dead",
   "0x00000000000000000000000000000000deadbeef"].contains(context.recipient)
};
