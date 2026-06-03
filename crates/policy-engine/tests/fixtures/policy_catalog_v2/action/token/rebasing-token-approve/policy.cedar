// blocks: an approval of a rebasing token whose balance shifts over time, so a fixed approval under-covers or an unlimited approval over-covers an ever-growing balance.
// why:    rebasing-token approval-scope threat class (e.g. stETH, aTokens) — a static allowance does not track a dynamic balance, leaving the user with a mis-scoped (often unbounded) spend grant.
// bucket: action/token  (precedence-won)
// tags: bucket:action cat:token enrichment method:token.metadata
// methods: token.metadata -> ../../../_methods/token.metadata.md
//
// Catalog I2 — warn when approving a rebasing token (e.g. stETH, aTokens); token.metadata enriches the rebasing flag.
// ENRICHMENT — dormant until the dispatcher serves `token.metadata`.
@id("rebasing-token-approve")
@severity("warn")
@reason("This is a rebasing token — its balance changes over time; review the approval scope")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has isRebasing
  && context.custom.isRebasing == true
};
