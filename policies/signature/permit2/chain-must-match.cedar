// Policy: block Permit2 signatures whose EIP-712 domain chain differs from the request chain.

@id("user/signature/permit2/chain-must-match")
@severity("deny")
@reason("Permit2 domain chainId does not match request chainId")
forbid (
  principal is Wallet,
  action == Action::"signature.permit2",
  resource is Protocol
)
when {
  context.domainChainId != context.requestChainId
};
