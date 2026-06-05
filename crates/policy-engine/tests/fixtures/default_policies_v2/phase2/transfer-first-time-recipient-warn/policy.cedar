// scopeball default v2 policy — demand signal: clipboard-hijack-changed-paste-
// address (clipper malware swaps the copied address right before Send; the user
// only checks first/last chars and misses it).
//
// The wallet cannot read the OS clipboard, so the raw "address was swapped"
// signal is not expressible. We catch the OUTCOME instead: a clipboard swap
// always points the transfer at an address this wallet has NEVER sent to
// before. So we warn (a one-beat pause, not a block) on a first-time recipient
// and ask the user to re-read the full address.
//
// "Have I ever sent to this recipient?" is recipient-interaction HISTORY, which
// lives outside the stateless policy engine. Same shape as
// transfer-blocklisted-recipient-deny: an external policy-rpc method
// (`external.recipient_first_seen`) reads the wallet's on-chain counterparty
// history host-side and projects a verdict into
// `context.custom.firstTimeRecipient`. The call is `optional`, so until the
// provider ships its absence fail-opens (the `has` guard is false → no-op)
// rather than SystemFail. This adds NO new Cedar state surface — the engine
// stays stateless and only compares a host-supplied Bool.
@id("transfer-first-time-recipient-warn")
@severity("warn")
@reason("You have never sent to this address before — re-read the full address in case it was swapped")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has firstTimeRecipient
  && context.custom.firstTimeRecipient
};
