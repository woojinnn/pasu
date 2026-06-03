// blocks: sending an outright ERC-20 transfer worth more than $10k of USD value.
// why:    irreversible large-value send — a mistaken/coerced transfer of a big
//         balance has no recourse; doubles as a travel-rule-style amount threshold.
// bucket: wallet/usd-cap  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap could-be-compliance:travel-rule
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// Catalog D3 — warn on an ERC-20 transfer worth more than $10k.
// `oracle.usd_value` enriches the transfer notional into `context.custom.transferUsd`.
// ENRICHMENT — dormant until the dispatcher serves `oracle.usd_value`.
@id("transfer-usd-cap")
@severity("warn")
@reason("Large transfer (over $10k) — confirm the amount and recipient before sending")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has transferUsd
  && context.custom.transferUsd.greaterThan(decimal("10000.0000"))
};
