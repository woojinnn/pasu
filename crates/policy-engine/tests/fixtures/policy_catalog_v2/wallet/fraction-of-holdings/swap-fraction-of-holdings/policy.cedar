// blocks: a swap whose input amount consumes more than half (>5000 bp) of the wallet's balance of the input token, without a deliberate confirmation.
// why:    swapping an outsized fraction of one asset concentrates slippage/MEV exposure and is a common drainer pattern (route the whole bag through a malicious pool); the fraction-of-holdings heuristic flags it pre-sign. (Fireblocks fraction-of-holdings policy.)
// bucket: wallet/fraction-of-holdings
// tags: bucket:wallet cat:fraction-of-holdings enrichment method:portfolio.input_fraction_bps action-family:fraction-of-holdings
// methods: portfolio.input_fraction_bps -> ../../../_methods/portfolio.input_fraction_bps.md
@id("swap-fraction-of-holdings")
@severity("warn")
@reason("This swap spends over half your balance of the input token — confirm before swapping")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has holdingsBp
  && context.custom.holdingsBp > 5000
};
