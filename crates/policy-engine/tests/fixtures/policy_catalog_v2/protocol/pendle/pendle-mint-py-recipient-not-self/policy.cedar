// blocks: minting Pendle PT+YT (mint_py) with the minted principal/yield tokens delivered to an address other than the signer
// why:    output-redirection — a mint_py with recipient != submitter routes the freshly-minted PT/YT to a third party, a fund-loss vector if the recipient was injected by a malicious frontend (Pendle V2 Router docs)
// bucket: protocol/pendle
// tags: bucket:protocol cat:pendle static venue:pendle action-family:recipient-not-self
@id("pendle-mint-py-recipient-not-self")
@severity("warn")
@reason("This PT/YT mint delivers to another address — confirm the recipient before signing")
forbid(principal, action == Yield::Action::"MintPy", resource)
when { context.recipient != context.meta.submitter };
