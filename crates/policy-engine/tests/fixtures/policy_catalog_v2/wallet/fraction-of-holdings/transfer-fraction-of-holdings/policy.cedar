// blocks: a single ERC-20 transfer that drains more than half (>5000 bp) of the wallet's balance of that token without a deliberate confirmation.
// why:    fat-finger / drainer mitigation — an oversized outflow relative to holdings is a hallmark of wallet-drain and mistaken-amount transfers; the fraction-of-holdings heuristic flags it before signing.
// bucket: wallet/fraction-of-holdings  (precedence-won)
// tags: bucket:wallet cat:fraction-of-holdings enrichment method:portfolio.input_fraction_bps
// methods: portfolio.input_fraction_bps -> ../../../_methods/portfolio.input_fraction_bps.md
// Catalog D6 — warn when a transfer moves more than 50% of the wallet's balance
// of that token. `portfolio.input_fraction_bps` enriches the fraction (bp of
// holdings). ENRICHMENT — dormant until the dispatcher serves it.
@id("transfer-fraction-of-holdings")
@severity("warn")
@reason("This transfer moves over half your balance of this token — confirm before sending")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has holdingsBp
  && context.custom.holdingsBp > 5000
};
