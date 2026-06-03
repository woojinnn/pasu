// blocks: a swap whose input sells more than half (>5000 bp) of the wallet's balance of the input token.
// why:    fat-finger / drainer mitigation — selling an oversized fraction of holdings in one swap is a hallmark of mistaken-amount and drain transactions; the fraction-of-holdings heuristic flags it before signing (source: rabby).
// bucket: action/swap
// tags: bucket:action cat:swap enrichment method:portfolio.input_fraction_bps action-family:swap
// methods: portfolio.input_fraction_bps -> ../../../_methods/portfolio.input_fraction_bps.md
// portfolio.input_fraction_bps enriches the input amount as bp of the wallet's
// balance of tokenIn (registered method — active).
@id("swap-input-fraction-of-holdings")
@severity("warn")
@reason("This swap sells over half your balance of the input token — confirm before proceeding")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has holdingsBp
  && context.custom.holdingsBp > 5000
};
