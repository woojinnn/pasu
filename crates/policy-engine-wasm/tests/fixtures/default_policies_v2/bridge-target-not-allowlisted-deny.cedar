// scopeball default v2 policy — explorer id: BRG-02 (funds sent to a bridge
// contract that is not on the known-good bridge allowlist).
//
// Fake bridge sites are a top bridging loss path ($520K LINK to a spoofed
// bridge; Stargate / Across UI impersonation is common). A bridge move with no
// dedicated decode lands as `Core::Unknown` with the destination contract as
// the parsed, COMPARABLE String field `context.target`. The control is a
// literal SET-MEMBERSHIP test: deny when `target` is NOT among the known-good
// bridge contracts — no enrichment / policy_rpc call is needed because the
// allowlist is config inlined as a Cedar set literal (design proposal F#2 —
// personalized / registry-backed supply via a Set<String> custom field is a
// later increment). The manifest stays pure ({id, schema_version, trigger}).
//
// Addresses are lowercase hex (lowering normalizes); the sample entries below
// are canonical mainnet bridge routers (Stargate, Across SpokePool, CCTP
// TokenMessenger). Unmatched Unknown calldata is also caught by the generic
// blind-sign warning (GEN-09), so a registry miss never silently passes.
@id("bridge-target-not-allowlisted-deny")
@severity("deny")
@reason("Bridge contract is not on the known-good bridge allowlist — possible fake bridge")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  !([
    "0x8731d54e9d02c286767d56ac03e8037c07e01e98",
    "0x5c7bcd6e7de5423a257d81b442095a1a6ced35c5",
    "0xbd3fa81b58ba92a82136038b25adec7066af3155"
  ].contains(context.target))
};
