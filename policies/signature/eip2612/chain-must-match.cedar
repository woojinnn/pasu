// Policy: block EIP-2612 permits whose EIP-712 domain chain differs from the request chain.

@id("user/signature/eip2612/chain-must-match")
@severity("deny")
@reason("EIP-2612 domain chainId does not match request chainId")
forbid (
  principal is Wallet,
  action == Action::"signature.eip2612",
  resource is Protocol
)
when {
  context.domainChainId != context.requestChainId
};
