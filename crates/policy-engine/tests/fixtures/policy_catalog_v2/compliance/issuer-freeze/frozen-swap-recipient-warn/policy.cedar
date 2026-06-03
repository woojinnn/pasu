// blocks: a swap whose output recipient is frozen on the OUTPUT token's issuer blocklist — soft warn.
// why:    issuer-freeze screening — routing swap proceeds (tokenOut) to a recipient the stablecoin issuer has frozen (Circle/Centre blocklist, Tether isBlackListed) is pointless and a flag: the recipient cannot move the received asset, so the proceeds would be stranded, and routing to a flagged destination is a counterparty-under-enforcement signal. Warn (not deny) — the recipient may be the signer's own to-be-cleared address, so surface for review. (Circle / Centre FiatTokenV2 isBlacklisted; Tether TetherToken isBlackListed.)
// bucket: compliance/issuer-freeze
// tags: bucket:compliance cat:issuer-freeze enrichment method:address.frozen could-be-compliance:true action-family:swap
// methods: address.frozen -> ../../../_methods/address.frozen.md
// dormant: until address.frozen is registered in method-catalog.json
//
// `address.frozen` views the issuer's on-chain blocklist for (asset=tokenOut, address=recipient).
// Asset-scoped to the swap OUTPUT token (the asset the recipient receives). ENRICHMENT —
// dormant until the dispatcher serves the method; the `context.custom has frozen` guard
// is inert until then.
@id("frozen-swap-recipient-warn")
@severity("warn")
@reason("Swap output recipient is frozen on the output token's issuer blocklist — review before swapping")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has frozen
  && context.custom.frozen == true
};
