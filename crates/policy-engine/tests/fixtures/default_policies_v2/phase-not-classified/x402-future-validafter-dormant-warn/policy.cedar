// BLOCKED-BY-ACTION: erc3009_transfer_with_auth surface not yet in scopeball schema (see x402 gap-spec)
// Source: EIP-3009 TransferWithAuthorization typed-data (x402 exact scheme). `validAfter`
//   is a unix-seconds Long on the authorization: the auth is NOT spendable before it, so a
//   `validAfter` set in the FUTURE means the user signs a bearer claim that lies DORMANT and
//   becomes spendable later ("sign now, payable in two weeks" — eco.com docs; EIP-3009
//   explicitly allows "schedule a transaction to be only valid in the future").
// Threat: a future-dated authorization is a deferred-execution bearer claim. Combined with a
//   far `validBefore` the dormant-replay window widens. The dormant-then-forgotten mechanic is
//   confirmatory with the $908,551 USDC 458-day-dormant theft (pink-drainer). We surface the
//   reservation to the user rather than letting a "scheduled / dormant payment" pass silently.
//
// Enrichment (one derived fact, server clock):
//   - `authStartHorizonSec` — seconds from the evaluation clock (now) to `validAfter`
//     (validAfter - now). `validAfter` is a Long on the context but the horizon needs the
//     sim-server clock, which Cedar lacks. > 0 ⇒ `validAfter` is in the future ⇒ the auth is
//     dormant / scheduled. DIRECT precedent: intent.validity_horizon_sec (validUntil - now);
//     here the sibling reads $.action.validAfter and returns the START horizon. Shares the
//     same horizon method as x402-auth-far-future-expiry-warn (start vs expiry projection).
//
// `custom` is optional, so the field is `has`-guarded (3-part). `authStartHorizonSec` is a
// Cedar Long (use `>`). The clock read lives in the sim-server fact host, so the call is
// `optional` and the policy fail-opens until it ships. warn (not deny): future-dating is a
// legitimate primitive (recurring/scheduled pay), so we inform rather than block.
@id("x402-future-validafter-dormant-warn")
@severity("warn")
@reason("This x402 payment authorization is not valid until a future time — you are signing a dormant claim that becomes spendable later. Confirm you intend a scheduled/deferred payment.")
forbid(principal, action == Token::Action::"Erc3009TransferWithAuth", resource)
when {
  context has custom
  && context.custom has authStartHorizonSec
  && context.custom.authStartHorizonSec > 0
};
