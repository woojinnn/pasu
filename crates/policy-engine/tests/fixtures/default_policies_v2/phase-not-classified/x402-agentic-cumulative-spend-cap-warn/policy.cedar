// BLOCKED-BY-ACTION: erc3009_transfer_with_auth surface not yet in scopeball schema (see x402 gap-spec)
// BLOCKED-BY-STATE: cross-request x402 payment accumulator (per-session / 24h window spend total +
//   recent (to,nonce) duplicate set) is not in scopeball wallet-state today. The single-request
//   `value`/`valueUsd` action fields cannot express a SUM across many prior authorizations.
//
// scopeball policy — autonomous-agent x402 "runaway": an AI agent signs EIP-3009
// TransferWithAuthorization payments (the x402 "exact" scheme) WITHOUT human-in-the-loop, so a
// looping / hallucinating / prompt-injected agent can emit many individually-micro but cumulatively
// wallet-draining authorizations. dev.to (l_x_1): "Loop continues until wallet is drained";
// builders ask for "Daily budget caps, endpoint whitelist, per-report spending limits, duplicate
// payment detection". GetOnStack: an 11-day recursive loop burned ~$47,000. thirdweb's
// "sign-once-settle-many" caches and re-uses a still-valid authorization; arxiv 2605.11781 observed
// 248 HTTP-layer grants settling to 1 on-chain transfer (idempotency gap amplifies the drain).
//
// No single-request field check can see this — the threat is the RUNNING TOTAL across requests
// (session / daily, optionally per pay-to endpoint) plus near-duplicate re-payment to the same
// (to, nonce). That requires a cross-request accumulator + timer/window state that scopeball does
// not have yet (closest precedents are the reducer pending-set folds intent.pending_cap_over_balance
// / intent.near_duplicate_pending and the windowed order.swap_count_24h, but NONE holds x402 payment
// history). So a NEW fact (x402_budget.window_spend_state) is proposed; until both the action surface
// and that state land, the call is optional -> fail-open and this warn no-ops.
//
// Two enriched Bools drive this (one record-output call):
//   - budgetExceeded     — true iff (accumulated window spend USD) + (this value's USD) crosses the
//                          configured session/daily cap. The host folds prior x402 payments in the
//                          window and adds the priced new `value`.
//   - duplicateRecent    — true iff a near-identical x402 payment (same `to`, and/or same `nonce`)
//                          was already authorized inside the recent dedup window (sign-once-settle-
//                          many / idempotency-gap re-pay).
// custom is optional, so each field is the 3-part has-guard; both are Bool (warn when true).
@id("x402-agentic-cumulative-spend-cap-warn")
@severity("warn")
@reason("This automated x402 payment pushes your session/daily x402 spending over the budget cap, or repeats a recent payment to the same recipient — an agent loop can drain your wallet through many small authorizations")
forbid(principal, action == Token::Action::"Erc3009TransferWithAuth", resource)
when {
  context has custom
  && ((context.custom has budgetExceeded && context.custom.budgetExceeded)
      || (context.custom has duplicateRecent && context.custom.duplicateRecent))
};
