// BLOCKED-BY-ACTION: erc3009_transfer_with_auth surface not yet in scopeball schema (see x402 gap-spec)
//
// scopeball policy — RESIDUAL x402 / EIP-3009 TransferWithAuthorization (or
// ReceiveWithAuthorization) decode notice: REPLACES the opaque
// Core::Unknown blind-sign warning (unknown-blind-sign-warning) with a
// meaning-bearing one once the typed-data is decoded into a payment action.
//
// Source / threat: the x402 "exact" scheme rides on an EIP-3009
// TransferWithAuthorization typed-data signature — the payer signs
// { from, to, value, validAfter, validBefore, nonce } and the facilitator
// later submits transferWithAuthorization(...) on-chain. TODAY scopeball has
// no erc3009 surface, so this typed-data is caught only as Core::Unknown
// (unknown-blind-sign-warning): the user sees a vague "you are blind-signing
// something" prompt and CANNOT see to / value / expiry. Worse, the
// coinspect / valkyrisec EIP-712 review shows an attacker can "inject arbitrary
// data in the message field ... displayed ... although not used to compute the
// signature", so a wallet's raw-message preview is itself untrustworthy. Adding
// the Erc3009TransferWithAuth surface lets sign-resolver decode the typed-data
// into a real payment action, after which the field-specific x402 policies
// (from-not-connected, near-full-balance, pay-to blocklist, micro-cap, far
// expiry, …) can fire on canonical, signature-bound fields — opacity replaced
// by meaning.
//
// This candidate is the RESIDUAL of that decode: when no deny/warn field-policy
// has matched, it still surfaces the canonical from / to / value / validBefore
// to the user (advice) instead of an unexplained blind-sign. It is the
// justification-of-surface policy: the same Erc3009TransferWithAuth tag that
// makes this fire is what removes the request from the Core::Unknown path, so
// this is a REPLACE of unknown-blind-sign-warning, not a duplicate.
//
// Demand: strong (x402 spec + first-person coinspect/valkyrisec vuln report —
// message-field injection makes the raw preview unreliable, so a structured
// decode is required to show payment intent honestly). Anticipatory: no direct
// x402 loss observed yet; grounded in the EIP-712 message-field-injection
// finding that proves preview-untrustworthiness.
//
// Pure action-field policy (no enrichment): `validBefore` is a NON-OPTIONAL
// base context Long (unix seconds; EIP-3009 authorization expiry) on the new
// Erc3009TransferWithAuth surface. Reading it (> 0, structurally true for any
// well-formed unix timestamp) makes this a residual notice that fires whenever
// the payment was successfully decoded, with from / to / value / validBefore
// carried in the request context for the user to read — no has-guard,
// no policy_rpc, no custom_context. The comparison uses the strict `>`
// operator (the convention's enumerated Long operator; cf. the cedar-only
// base-Long precedent permit2-sign-allowance-far-expiry-warn:
// `context.expiresAt > 2000000000`). Warn (a payment, surfaced for review),
// never deny: the deny/warn field-policies own the risky branches.
@id("x402-unknown-blind-sign-replaces-warn")
@severity("warn")
@reason("This is an x402 (EIP-3009) payment authorization, not an opaque blind signature — review the payee (to), amount (value), and expiry (validBefore) before signing")
forbid(principal, action == Token::Action::"Erc3009TransferWithAuth", resource)
when {
  context.validBefore > 0
};
