// BLOCKED-BY-ACTION: erc3009_transfer_with_auth surface not yet in scopeball schema (see x402 gap-spec)
//
// scopeball policy — x402 (EIP-3009 TransferWithAuthorization / ReceiveWithAuthorization)
// payment authorization whose `validBefore` sits far in the future.
//
// An EIP-3009 authorization is a BEARER instrument: once signed, the signature
// itself is the payment — anyone who holds it can submit transferWithAuthorization
// any time up to `validBefore` and pull exactly `value` USDC from the payer. The
// x402 exact scheme expects a SHORT window (payin.com guidance:
// `validBefore: Date.now()/1000 + 3600`, i.e. ~1h). A far-future `validBefore`
// (months/years out; SlowMist measured a permit deadline of year 56300 ≈
// "practically unlimited") turns a forgotten signature into a time-bomb a
// signature-harvesting attacker can replay at will. Direct precedent for the harm:
// pink-drainer drained $908,551 USDC 458 days after the approval was signed
// (coinmarketcap academy) — far-future expiry = a replayable bearer the user forgot.
//
// `validBefore` is an absolute unix-seconds Long on the base context, but the
// danger is the HORIZON `validBefore - now`, and Cedar has no server clock. So the
// horizon is computed off-Cedar by the fact host (DIRECT precedent:
// intent.validity_horizon_sec = validUntil - now). `auth.validity_horizon_sec` is
// its sibling reading `$.action.validBefore`; it emits `authHorizonSec` (Long).
// `custom` is optional, so the field is read with the 3-part has-guard. The call
// is `optional` (fail-open) until the method ships. Threshold is user config; this
// slice inlines 86400 (warn when the authorization stays valid for >24h, far above
// the ~1h x402 window). People sometimes want longer windows, hence warn not deny.
@id("x402-auth-far-future-expiry-warn")
@severity("warn")
@reason("This x402 payment authorization stays valid far longer than the recommended ~1 hour window — a long-lived signed authorization is a bearer instrument anyone holding the signature can redeem until it expires")
forbid(principal, action == Token::Action::"Erc3009TransferWithAuth", resource)
when {
  context has custom
  && context.custom has authHorizonSec
  && context.custom.authHorizonSec > 86400
};
