// BLOCKED-BY-ACTION: erc3009_transfer_with_auth surface not yet in scopeball schema (see x402 gap-spec)
//
// scopeball policy — x402 / EIP-3009 TransferWithAuthorization (or
// ReceiveWithAuthorization) whose signed `from` (the authorizer/payer) is NOT
// the connected wallet, or is the zero / malformed address.
//
// Source / threat: the x402 "exact" scheme rides on an EIP-3009
// TransferWithAuthorization typed-data signature: the payer signs
// { from, to, value, validAfter, validBefore, nonce } and the facilitator
// later submits transferWithAuthorization(...) on-chain. EIP-3009 requires the
// contract to `ecrecover` the signature back to `from` AND to reject the zero
// address; the x402 spec likewise says the verifier MUST confirm the signature
// "recovers to the authorization.from address". A well-formed authorization the
// user actually controls therefore has `from == the connected wallet`. When the
// signed `from` differs from the connected/owning wallet ($.root.from, exposed
// to Cedar as `principal.address`) — or is the zero address / malformed — the
// signature is anomalous: a relayed / confused / spoofed authorization the user
// did not author. Warn before signing.
//
// Anticipatory: no first-person x402-native from-spoof loss is observed yet;
// this is a defensive invariant grounded in the EIP-3009 ecrecover/zero-address
// rule and the x402 from-recovery requirement. Low-cost because it is a pure
// action-field comparison.
//
// This is a pure action-field policy (no enrichment): `from` is a NON-OPTIONAL
// base context String on the new Erc3009TransferWithAuth surface and
// `principal.address` is the connected Wallet entity address, so no has-guard,
// no policy_rpc, and no custom_context are needed. People do not legitimately
// x402-authorize FROM an address other than their connected wallet, hence warn.
@id("x402-from-not-connected-wallet-warn")
@severity("warn")
@reason("This x402 payment authorization is signed FROM an address that is not your connected wallet (or is an empty/invalid address) — a malformed or relayed signature you may not have intended")
forbid(principal, action == Token::Action::"Erc3009TransferWithAuth", resource)
when {
  context.from != principal.address
  || context.from == "0x0000000000000000000000000000000000000000"
};
