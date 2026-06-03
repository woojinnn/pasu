// blocks: a swap routed through an aggregator/router contract not on the user's allowlist
// why:    confining interactions to vetted router contracts (Coinbase CDP contract-allowlist) stops a swap being sent to a look-alike or malicious aggregator address; `venue.router` is the aggregator contract being called
// bucket: wallet/venue-allowlist
// tags: bucket:wallet cat:venue-allowlist static action-family:contract-allowlist
// `venue.router` is OPTIONAL (only set for aggregator_route venues) — has-guarded.
// A swap whose venue carries no router (direct AMM) does NOT trip this rule.
// User-configured allowlist (lowercase 0x; placeholders below). Pure-static.
@id("interaction-contract-allowlist")
@severity("deny")
@reason("Swap router contract is not on your allowlist — interaction blocked")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context.venue has router &&
  !(["0x000000000000000000000000000000000000dead", "0x1111111111111111111111111111111111111111"].contains(context.venue.router))
};
