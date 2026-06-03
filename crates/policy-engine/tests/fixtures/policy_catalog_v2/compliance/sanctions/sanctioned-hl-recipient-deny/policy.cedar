// blocks: a Hyperliquid withdraw to a destination EVM address on an OFAC SDN / EU / UN sanctions list.
// why:    sanctions screening — withdrawing funds off the venue to a sanctioned destination exfiltrates value to a blocked party; HL withdraw is the venue fund-exit, screen its destination (OFAC SLS / Treasury SDN list). DENY-CLOSED venue flow.
// bucket: compliance/sanctions
// tags: bucket:compliance cat:sanctions enrichment method:address.sanctions could-be-compliance:true venue:hyperliquid action-family:hyperliquid
// methods: address.sanctions -> ../../../_methods/address.sanctions.md
// dormant: until address.sanctions is registered in method-catalog.json
//
// `address.sanctions` screens the HL withdraw destination against OFAC/EU/UN
// lists. ENRICHMENT — dormant until the dispatcher serves the method.
@id("sanctioned-hl-recipient-deny")
@severity("deny")
@reason("Hyperliquid withdraw destination is on a sanctions list (OFAC/EU/UN) — withdraw blocked")
forbid(principal, action == HyperliquidCore::Action::"HlWithdraw", resource)
when {
  context has custom
  && context.custom has sanctioned
  && context.custom.sanctioned == true
};
