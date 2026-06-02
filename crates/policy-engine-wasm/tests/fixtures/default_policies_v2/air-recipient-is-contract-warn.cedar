// scopeball default v2 policy — explorer id: AIR-08 (claim recipient is a
// contract rather than my own EOA — a possible drainer relay/proxy).
//
// METHOD-BACKED external/oracle read (Aggregator type 9; sv = LI). EOA-vs-
// contract is decided by whether the recipient address has deployed code
// (codehash / eth_getCode) — a LIVE on-chain lookup that sits OUTSIDE the wallet
// snapshot, so it is filled by external info, not state. A NEW
// `external.recipient_is_contract` fact runs the codehash check and projects the
// verdict into `context.custom.recipientIsContract` (Bool). Defined-not-wired →
// `optional`, so its absence fail-opens until the external provider lands. Warn
// (plausible, not certain) when the recipient is a contract.
@id("air-recipient-is-contract-warn")
@severity("warn")
@reason("This airdrop pays out to a contract rather than your wallet — it may be a drainer relay")
forbid(principal, action == Airdrop::Action::"Claim", resource)
when {
  context has custom
  && context.custom has recipientIsContract
  && context.custom.recipientIsContract
};
