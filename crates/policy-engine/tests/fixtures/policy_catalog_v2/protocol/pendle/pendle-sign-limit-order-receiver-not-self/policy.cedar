// blocks: signing a Pendle limit order whose receiver is not the maker, routing the filled proceeds to a third party
// why:    proceeds-redirection on a signed order — a maker-signed Pendle limit order with receiver != maker silently sends the output to another address; review the destination before signing (Pendle V2 limit-order docs)
// bucket: protocol/pendle
// tags: bucket:protocol cat:pendle static venue:pendle action-family:recipient-not-self
@id("pendle-sign-limit-order-receiver-not-self")
@severity("warn")
@reason("This Pendle limit order pays out to an address other than the maker — confirm the receiver before signing")
forbid(principal, action == Yield::Action::"SignLimitOrder", resource)
when { context.receiver != context.maker };
