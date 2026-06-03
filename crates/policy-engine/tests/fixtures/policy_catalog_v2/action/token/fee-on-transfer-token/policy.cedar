// blocks: a transfer of a fee-on-transfer token where the recipient silently receives less than the displayed amount (the token skims a fee).
// why:    fee-on-transfer / deflationary token threat class — naive "amount sent == amount received" assumptions break, causing under-delivery and downstream accounting/slippage failures.
// bucket: action/token  (precedence-won)
// tags: bucket:action cat:token enrichment method:token.metadata
// methods: token.metadata -> ../../../_methods/token.metadata.md
//
// Catalog I1 — warn when transferring a fee-on-transfer token; token.metadata enriches feeBps.
// ENRICHMENT — dormant until the dispatcher serves `token.metadata`.
@id("fee-on-transfer-token")
@severity("warn")
@reason("This token charges a transfer fee — the recipient will receive less than the amount shown")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has feeBps
  && context.custom.feeBps > 0
};
