// Policy: block unmatched EIP-712 signatures whose domain chain differs from the request chain.

@id("user/signature/eip712-other/chain-must-match")
@severity("deny")
@reason("EIP-712 domain chainId does not match request chainId")
forbid (
  principal is Wallet,
  action == Action::"signature.eip712_other",
  resource is Protocol
)
when {
  context.domainChainId != context.requestChainId
};
