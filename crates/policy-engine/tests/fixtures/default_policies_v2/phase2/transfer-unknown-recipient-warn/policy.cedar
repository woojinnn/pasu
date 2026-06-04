// scopeball default v2 policy — demand signal: first-time-recipient-confirm-
// pause (no warning when sending to a brand-new address never used before, so
// wrong/scam addresses slip through; industry has not standardized this).
//
// We warn (a one-beat pause, not a block) when an ERC20 transfer targets a
// recipient this wallet has NEVER sent to before, so the user re-reads the full
// address before signing. Recipients you have used before pass silently → low
// friction; only the unfamiliar address gets the pause.
//
// "Have I ever sent to this recipient?" is recipient-interaction HISTORY, which
// lives OUTSIDE the stateless policy engine (the signal itself flags persistent
// wallet state as the missing piece). We follow the exact shape of
// transfer-blocklisted-recipient-deny: an external policy-rpc method
// (`external.recipient_seen_before`) keeps the send-history host-side and
// projects a verdict into `context.custom.recipientFirstSeen`. The Cedar engine
// adds NO new state surface — it only compares a host-supplied Bool. The call is
// `optional`, so until the provider ships its absence fail-opens (the `has`
// guard is false → no-op) rather than SystemFail; it activates automatically
// once the provider lands.
@id("transfer-unknown-recipient-warn")
@severity("warn")
@reason("You have never sent to this address before — re-read the full address before sending")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientFirstSeen
  && context.custom.recipientFirstSeen
};
