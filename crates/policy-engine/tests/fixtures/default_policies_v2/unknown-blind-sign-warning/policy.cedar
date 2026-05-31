// scopeball default v2 policy — explorer id: GEN-09 (blind / opaque signature).
//
// Warn whenever an action could not be decoded by any manifest and therefore
// surfaces as `Core::Action::"Unknown"` (opaque EIP-712, raw `eth_sign`, or
// calldata no adapter matched). Signature-phishing leans on exactly these
// undecodable requests, so the wallet should flag them for a human look.
//
// This is a pure action-shape policy (Aggregator type 8 — Decode
// Introspection): the verdict depends only on the decoded action's *identity*,
// not on any enriched fact. The manifest therefore declares no `policy_rpc`
// calls and no `custom_context`.
@id("unknown-blind-sign-warning")
@severity("warn")
forbid(principal, action == Core::Action::"Unknown", resource);
