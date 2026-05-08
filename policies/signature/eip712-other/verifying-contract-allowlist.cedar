// Policy: only allow unmatched EIP-712 signatures for known verifying contracts.

@id("user/signature/eip712-other/verifying-contract-allowlist")
@severity("deny")
@reason("EIP-712 verifying contract is not on the allowlist")
forbid (
  principal is Wallet,
  action == Action::"signature.eip712_other",
  resource is Protocol
)
unless {
  ["0x9999999999999999999999999999999999999999"].contains(context.base.verifyingContract)
};
