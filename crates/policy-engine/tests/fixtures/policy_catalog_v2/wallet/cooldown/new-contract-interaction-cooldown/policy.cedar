// blocks: interacting with an undecoded (catch-all) contract target that was first deployed/seen inside the cooldown window.
// why:    Safe{Wallet} Guardrail-style new-contract cooldown — a freshly-deployed, unrecognised target (no manifest matched) is the textbook just-deployed drainer; a grace window forces a deliberate second look before signing into unknown code (fresh-contract threat class).
// bucket: wallet/cooldown
// tags: bucket:wallet cat:cooldown enrichment method:address.activity action-family:cooldown
// methods: address.activity -> ../../../_methods/address.activity.md
// dormant: until address.activity is registered in method-catalog.json
//
// Cooldown — warn when the catch-all target was first seen within the last
// 7 days (604800s). `address.activity` enriches the target first-seen timestamp.
// ENRICHMENT — dormant.
@id("new-contract-interaction-cooldown")
@severity("warn")
@reason("This contract was deployed/seen very recently — wait out the cooldown before interacting with new code")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has targetFirstSeenTs
  && context.custom.targetFirstSeenTs > (context.meta.submittedAt - 604800)
};
