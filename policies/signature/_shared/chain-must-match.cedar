// Policy: block signatures whose EIP-712 domain chain differs from the request chain.

@id("user/signature/permit2/chain-must-match")
@severity("deny")
@reason("Permit2 domain chainId does not match request chainId")
forbid (
  principal is Wallet,
  action == Action::"signature.permit2",
  resource is Protocol
)
when {
  context.base.domainChainId != context.base.requestChainId
};

@id("user/signature/eip2612/chain-must-match")
@severity("deny")
@reason("EIP-2612 domain chainId does not match request chainId")
forbid (
  principal is Wallet,
  action == Action::"signature.eip2612",
  resource is Protocol
)
when {
  context.base.domainChainId != context.base.requestChainId
};

@id("user/signature/eip712-other/chain-must-match")
@severity("deny")
@reason("EIP-712 domain chainId does not match request chainId")
forbid (
  principal is Wallet,
  action == Action::"signature.eip712_other",
  resource is Protocol
)
when {
  context.base.domainChainId != context.base.requestChainId
};
