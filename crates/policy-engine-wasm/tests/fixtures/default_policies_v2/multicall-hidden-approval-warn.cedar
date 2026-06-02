// scopeball default v2 policy — explorer id: GEN-16 (an approval hidden inside a
// benign-looking bundle).
//
// Drainer bundles wrap a token grant inside an innocuous-looking multicall ("claim
// your reward") so the user sees the friendly outer call and never notices the
// `approve` / `setApprovalForAll` riding along. The reducer projects each child
// `ActionBody` to a flat `Set<MulticallChildSummary>` of `(domain, action)` tags
// on the base context (`context.children`) — Cedar can read that set directly
// (Aggregator type 8 — Decode Introspection), so this is a pure action-shape
// policy with NO `policy_rpc` and NO `custom_context`.
//
// Warn when the bundle contains any token-grant child. Per-child detail
// (spender, amount) is evaluated separately — each child is also dispatched as
// its own envelope — so this policy only needs the (domain, action) shape to flag
// the bundle for a human look.
@id("multicall-hidden-approval-warn")
@severity("warn")
@reason("This bundle quietly includes a token-approval call — check what you are granting")
forbid(principal, action == Core::Action::"Multicall", resource)
when {
  context.children.containsAny([
    { domain: "token", action: "erc20_approve" },
    { domain: "token", action: "permit2_approve" },
    { domain: "token", action: "nft_approve" },
    { domain: "token", action: "nft_set_approval_for_all" }
  ])
};
