// blocks: an ERC-20 approval whose amount exceeds half (>5000 bp) of the wallet's balance of that token, granting a spender claim over an outsized share of holdings.
// why:    a spend allowance sized to a large fraction of holdings is the lever a malicious spender pulls later to drain the wallet; gating the approval at sign time bounds the blast radius. (EIP-7715 scoped-permission rationale.)
// bucket: wallet/fraction-of-holdings
// tags: bucket:wallet cat:fraction-of-holdings enrichment method:portfolio.input_fraction_bps action-family:fraction-of-holdings
// methods: portfolio.input_fraction_bps -> ../../../_methods/portfolio.input_fraction_bps.md
@id("approval-fraction-of-holdings")
@severity("warn")
@reason("This approval grants a spender over half your balance of this token — confirm the allowance")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has holdingsBp
  && context.custom.holdingsBp > 5000
};
