// blocks: re-approving a spender that already holds a non-zero allowance (the dApp is raising an existing grant)
// why:    ERC-20 approve race / front-run — changing a live allowance without first zeroing it lets the old + new allowance both be spent (the classic SWC-114-style approve double-spend)
// bucket: action/approval  (precedence-won)
// tags: bucket:action cat:approval enrichment method:approval.allowance
// methods: approval.allowance -> ../../../_methods/approval.allowance.md
// Catalog A6 — warn when approving a spender that already holds an allowance.
//
// Re-approving a spender with a non-zero existing allowance is the classic
// ERC-20 approve race (and usually means the dApp wants MORE than it already
// has). `approval.allowance` enriches whether a non-zero allowance already
// exists. ENRICHMENT — dormant until the dispatcher serves `approval.allowance`.
// Threat refs: ERC-20 approve front-run; speedrunethereum approve pattern.
@id("approve-existing-allowance")
@severity("warn")
@reason("This spender already has an allowance — revoke-to-zero first, or confirm you intend to raise it")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has hasExistingAllowance
  && context.custom.hasExistingAllowance == true
};
