// scopeball default v2 policy — explorer id: GEN-11 (transfer to a known-
// malicious / sanctioned recipient).
//
// Deny a transfer whose `recipient` is flagged by an external address blocklist
// (Aggregator type 9 — External Fact Lookup; OFAC/known-drainer feeds live
// outside wallet state). The manifest's `external.address_blocklisted` policy_rpc
// call projects the verdict into `context.custom.recipientBlocklisted`.
//
// The blocklist provider is an external policy-rpc host method (mock for this
// slice); until it ships, the call is `optional`, so its absence fail-opens —
// `context.custom has recipientBlocklisted` is false and the policy no-ops
// rather than SystemFail. It activates automatically once the provider lands.
@id("transfer-blocklisted-recipient-deny")
@severity("deny")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientBlocklisted
  && context.custom.recipientBlocklisted
};
