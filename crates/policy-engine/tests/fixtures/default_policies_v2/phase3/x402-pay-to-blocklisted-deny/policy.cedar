// BLOCKED-BY-ACTION: erc3009_transfer_with_auth surface not yet in scopeball schema (see x402 gap-spec)
//
// scopeball policy — x402 (EIP-3009 TransferWithAuthorization / ReceiveWithAuthorization)
// off-chain payment signature whose pay-to address `to` sits on an external
// reputation blocklist.
//
// In x402 the `to` (payee / resource-server pay-to) is cryptographically bound
// into the EIP-712 authorization, so the facilitator "cannot modify the amount or
// destination" (x402 spec). The threat is at signing time: a "sign to verify /
// login"-disguised phishing dapp sets `to` to a scam address, or a phishing site
// intercepts the genuine HTTP 402 response and swaps the recipient (valkyrisec),
// and the user — who never reads the raw EIP-712 JSON — signs an authorization
// paying `value` to the attacker exactly. Real x402-native incident: 402bridge
// (2025-10-28) drained ~$17,693 USDC across 200+ wallets via excessive
// authorization (forklog/crypto-economy/GoPlus); GoPlus' x402 audit found
// "Excessive Authorization (owner can drain funds)" the single most common flaw.
// Confirmatory: a look-alike `to` drained $3.047M USDC (2025-09-11, cryptoslate).
//
// Same external reputation signal as transfer-blocklisted-recipient-deny
// [erc20_transfer] and nft-transfer-blocklisted-recipient-deny [nft_transfer],
// but those fire on a DIFFERENT action.tag and never reach the erc3009 surface.
// This is the per-surface NET-NEW: the SAME `external.address_blocklisted` fact,
// re-bound from `$.action.recipient` to the x402 pay-to `$.action.to`.
//
// `external.address_blocklisted` returns a Bool projected here into the custom
// slot `payToBlocklisted`. `custom` is optional, so it carries the has-guard
// triad. The call is `optional` (fail-open): the DENY no-ops until the external
// reputation feed ships, and never blocks a legitimate payment on feed outage.
@id("x402-pay-to-blocklisted-deny")
@severity("deny")
@reason("The pay-to address in this x402 payment signature is on a scam blocklist — signing would pay exactly this amount to a known-malicious recipient")
forbid(principal, action == Token::Action::"Erc3009TransferWithAuth", resource)
when {
  context has custom
  && context.custom has payToBlocklisted
  && context.custom.payToBlocklisted
};
