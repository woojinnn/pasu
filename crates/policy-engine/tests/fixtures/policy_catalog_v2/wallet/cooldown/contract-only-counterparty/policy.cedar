// blocks: a swap routed through a venue whose router address has no deployed code (resolves to an EOA, not a contract).
// why:    Coinbase-style "interact only with real contracts" guardrail — a swap router that is an EOA (or an undeployed CREATE2 target) is never a legitimate AMM; an EOA counterparty here signals a spoofed / not-yet-deployed router (fake-venue threat class).
// bucket: wallet/cooldown
// tags: bucket:wallet cat:cooldown enrichment method:address.activity action-family:cooldown
// methods: address.activity -> ../../../_methods/address.activity.md
// dormant: until address.activity is registered in method-catalog.json
//
// Cooldown — warn when the swap venue router is NOT a deployed contract.
// `address.activity` enriches the router contract-ness flag. ENRICHMENT — dormant.
@id("contract-only-counterparty")
@severity("warn")
@reason("Swap router is not a deployed contract (looks like an EOA) — this is not a legitimate venue")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has venueIsContract
  && context.custom.venueIsContract == false
};
