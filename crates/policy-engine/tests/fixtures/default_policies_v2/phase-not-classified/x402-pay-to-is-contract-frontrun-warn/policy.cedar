// BLOCKED-BY-ACTION: erc3009_transfer_with_auth surface not yet in scopeball schema (see x402 gap-spec)
//
// scopeball policy — x402 EIP-3009 TransferWithAuthorization signed with a CONTRACT
// pay-to (`to`). transferWithAuthorization carries no front-running protection: an
// attacker watching the mempool — or, in x402, reading the PAYMENT-SIGNATURE HTTP
// header — can submit the authorization before the intended facilitator wrapper,
// so the payee's deposit logic is bypassed and funds can be mis-credited / locked
// (EIP-3009: "an attacker watching the transaction pool ... front-run the
// transferWithAuthorization ... receiveWithAuthorization prevents this by ...
// the caller is the payee"; arxiv 2605.11781: x402ExactPermit2Proxy "omits caller
// restriction in settle() ... any observer ... can submit the transfer first";
// payin.com / agentpaytrend corroborate). The EIP recommends receiveWithAuthorization
// (which binds caller == payee) precisely WHEN the payee is a contract. So: warn
// when the x402 pay-to is a deployed contract — the front-runnable Transfer variant
// was used where the Receive variant was indicated.
//
// Enrichment (one reused external fact, fail-open):
//   - payToIsContract — `external.recipient_is_contract` with `address` bound to
//                       `$.action.to` (live eth_getCode codehash). Bool: true iff the
//                       pay-to address has deployed bytecode. This is a LIVE on-chain
//                       lookup outside the wallet snapshot (external info, not state),
//                       so the call is `optional` and the warn no-ops until it ships.
//
// `custom` is optional => 3-part has-guard. Anticipatory (no x402-native incident
// observed yet); arxiv 2605.11781 reproduced the caller-unbound settle live. warn,
// not deny — a contract payee is legitimate when the facilitator uses the Receive
// variant; the signal is that THIS signature used the front-runnable Transfer one.
@id("x402-pay-to-is-contract-frontrun-warn")
@severity("warn")
@reason("This x402 payment authorization pays a contract using the front-runnable transferWithAuthorization variant — anyone who sees the signature can submit it first; the receiveWithAuthorization variant (caller must be the payee) is the safe form for contract recipients")
forbid(principal, action == Token::Action::"Erc3009TransferWithAuth", resource)
when {
  context has custom
  && context.custom has payToIsContract
  && context.custom.payToIsContract
};
