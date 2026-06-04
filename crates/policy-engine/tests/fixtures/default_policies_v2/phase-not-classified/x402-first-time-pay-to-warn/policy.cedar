// BLOCKED-BY-ACTION: erc3009_transfer_with_auth surface not yet in scopeball schema (see x402 gap-spec)
// scopeball policy — x402 (EIP-3009 TransferWithAuthorization / ReceiveWithAuthorization)
// off-chain payment authorization whose `to` (payee / pay-to) is an address this
// wallet has NEVER paid before. A first-seen pay-to can be a recipient-swap /
// "sign-to-verify" phishing substitution (valkyrisec recipient-swap; metamask: an
// off-chain signature lets the dapp use the signed message "at a time of their
// choosing"). Warn so the user re-confirms the destination — a softer net than the
// blocklist DENY (external.address_blocklisted).
//
// Same threat class & same fact as transfer-first-time-recipient-warn (erc20_transfer),
// but that policy keys on a DIFFERENT action tag and never fires on the EIP-3009 sig
// path. This is a per-surface NET-NEW: external.recipient_first_seen re-bound to the
// x402 pay-to (recipient <- $.action.to, from <- $.root.from), its `first_time` Bool
// projected into context.custom.firstTimePayTo. `custom` is optional, so the 3-part
// has-guard is required; the external counterparty-history read is optional → the
// warn fail-opens (no-ops) until both the action surface and the fact ship.
@id("x402-first-time-pay-to-warn")
@severity("warn")
@reason("This x402 payment goes to an address this wallet has never paid before — re-check the destination before signing")
forbid(principal, action == Token::Action::"Erc3009TransferWithAuth", resource)
when {
  context has custom
  && context.custom has firstTimePayTo
  && context.custom.firstTimePayTo
};
