// blocks: a fused approve+swap bundle where the approval and swap legs together form a drainer flow
// why:    approve-then-swap drainer — a single batch that approves a token then swaps it lets an attacker route funds into their own pool/router in one signature (batch-obfuscation / malicious-router threat class)
// bucket: action/batch  (precedence-won)
// tags: bucket:action cat:batch static
//
// Catalog H4 — warn on the classic approve+swap fused bundle.
// Approve then swap in one tx is a common drainer shape (approve the attacker,
// swap into their pool). OUTER scope: both legs present in the child set. Pure-static.
@id("batch-mixed-approve-swap")
@severity("warn")
@reason("This batch fuses a token approval with a swap — a common drainer pattern; review both legs")
forbid(principal, action == Core::Action::"Multicall", resource)
when {
  context.children.contains({ domain: "token", action: "erc20_approve" })
  && context.children.contains({ domain: "amm", action: "swap" })
};
