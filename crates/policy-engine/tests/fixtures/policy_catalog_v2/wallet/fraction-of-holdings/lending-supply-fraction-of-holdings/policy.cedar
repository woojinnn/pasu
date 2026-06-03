// blocks: a lending supply that deposits more than half (>5000 bp) of the wallet's balance of the asset into a money-market position.
// why:    moving an outsized fraction of holdings into a lending venue concentrates protocol/oracle/freeze risk on one position and reduces liquid reserves; warning at sign time lets the user confirm the concentration. (Fireblocks fraction-of-holdings policy.)
// bucket: wallet/fraction-of-holdings
// tags: bucket:wallet cat:fraction-of-holdings enrichment method:portfolio.input_fraction_bps action-family:fraction-of-holdings
// methods: portfolio.input_fraction_bps -> ../../../_methods/portfolio.input_fraction_bps.md
@id("lending-supply-fraction-of-holdings")
@severity("warn")
@reason("This supply deposits over half your balance of the asset — confirm before supplying")
forbid(principal, action == Lending::Action::"Supply", resource)
when {
  context has custom
  && context.custom has holdingsBp
  && context.custom.holdingsBp > 5000
};
