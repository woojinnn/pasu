// dambi default v2 policy — demand signal: irreversible-typo-fatfinger-send
// (Send Safety Set). Coinbase/MetaMask/ENS/Bitdeer all warn that a confirmed
// transfer is irreversible; the most certain way to lose funds permanently is a
// send to the burn / zero address.
//
// `recipient` is a base, non-optional `String` field of
// `Token::Erc20TransferContext` (same shape as swap-recipient-not-self-deny),
// so this judges with NO enrichment and NO policy_rpc — it just compares the
// recipient against the known burn-address literals. Addresses are lowercase
// hex on both sides (host-normalized), so exact String equality / Set membership
// holds. The zero address and 0x…dead have no key holder: a transfer there is
// permanent loss, hence `deny` (a hard block, not a one-beat warn).
//
// NOTE (dedup): the originating signal also bundled a "first-time recipient,
// pause" warn. That half duplicates the existing draft
// transfer-unknown-recipient-warn (same threat, severity, and external history
// Bool) and is intentionally NOT carried here; this policy keeps only the novel
// burn-address clause.
@id("send-first-time-or-burn-recipient-warn")
@severity("deny")
@reason("This send goes to a burn address (0x000…000 / 0x…dead) — funds sent there are permanently lost")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  ["0x0000000000000000000000000000000000000000",
   "0x000000000000000000000000000000000000dead"].contains(context.recipient)
};
