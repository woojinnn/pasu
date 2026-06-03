// blocks: a Pendle PT swap that delivers the output PT/token to an address other than the signer
// why:    output-redirection — a PT swap with recipient != submitter sends the swap proceeds to a third party, a fund-loss vector if the recipient was injected by a malicious frontend (Pendle V2 Router docs)
// bucket: protocol/pendle
// tags: bucket:protocol cat:pendle static venue:pendle action-family:recipient-not-self
@id("pendle-pt-swap-recipient-not-self")
@severity("warn")
@reason("This PT swap sends its output to another address — confirm the recipient before signing")
forbid(principal, action == Yield::Action::"PtSwap", resource)
when { context.recipient != context.meta.submitter };
