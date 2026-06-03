// blocks: silently handing your governance voting power to an unknown/malicious delegatee
// why:    governance takeover via ERC20Votes delegate(address) — a hostile delegatee votes your weight (governance-attack threat class)
// bucket: action/permission  (precedence-won; cross-cutting attrs preserved as tags)
// tags: bucket:action cat:permission static could-be:protocol-if-scoped
// Catalog G3 — confirm before delegating governance voting power (ERC20Votes
// `delegate(address)` used by UNI/COMP/ENS). A malicious delegatee can vote your
// weight. Pure-static.
@id("governance-delegate-confirm")
@severity("warn")
@reason("Delegating your governance voting power — confirm the delegatee")
forbid(principal, action == Airdrop::Action::"Delegate", resource);
