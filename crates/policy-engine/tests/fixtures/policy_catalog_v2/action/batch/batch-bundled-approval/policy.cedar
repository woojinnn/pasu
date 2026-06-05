// blocks: an ERC-20 approval slipping through unreviewed because it is bundled inside a multi-call batch
// why:    bundled-approval drainer — a hidden token approval among many calls grants an attacker spender allowance the user never scrutinized (malicious calldata / batch-obfuscation threat class)
// bucket: action/batch  (precedence-won)
// tags: bucket:action cat:batch static
//
// Catalog H3 — warn when a batch hides an ERC-20 approval among other calls.
// A bundled approval is easy to miss when reviewing a multi-call. OUTER scope:
// inspects the child summary set `{domain, action}`. Pure-static.
@id("batch-bundled-approval")
@severity("warn")
@reason("This batch contains a token approval bundled with other calls — review the approval target")
forbid(principal, action == Core::Action::"Multicall", resource)
when {
  context.children.contains({ domain: "token", action: "erc20_approve" })
};
