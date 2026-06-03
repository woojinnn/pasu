// blocks: an ERC-20 transfer that sweeps almost the entire balance (>90% of holdings) of that token — a hard deny tier above the softer warn-on-half rule.
// why:    near-total balance sweeps are the signature of an active wallet drain; at the >9000-bp tier the user should be hard-stopped, not merely warned. (Fireblocks fraction-of-holdings policy.)
// bucket: wallet/fraction-of-holdings
// tags: bucket:wallet cat:fraction-of-holdings enrichment method:portfolio.input_fraction_bps action-family:fraction-of-holdings
// methods: portfolio.input_fraction_bps -> ../../../_methods/portfolio.input_fraction_bps.md
@id("transfer-fraction-deny-tier")
@severity("deny")
@reason("This transfer sweeps over 90% of your balance of this token — blocked")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has holdingsBp
  && context.custom.holdingsBp > 9000
};
