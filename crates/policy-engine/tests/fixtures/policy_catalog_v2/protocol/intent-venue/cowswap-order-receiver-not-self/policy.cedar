// blocks: signing a CowSwap (intent) order whose receiver is not the signer, routing the bought tokens to a third party
// why:    proceeds-redirection on a signed intent — a CowSwap order with receiver != submitter is a solver-fillable mandate that delivers the buy-side tokens elsewhere; a malicious frontend can inject an attacker receiver the user never reviews (CoW Protocol order docs)
// bucket: protocol/intent-venue
// tags: bucket:protocol cat:intent-venue static venue:cow_swap action-family:recipient-not-self
@id("cowswap-order-receiver-not-self")
@severity("warn")
@reason("This intent order pays out to an address other than you — confirm the receiver before signing")
forbid(principal, action == Amm::Action::"SignIntentOrder", resource)
when { context.recipient != context.meta.submitter };
