// scopeball default v2 policy — explorer id: GEN-17 (signature whose chainId is
// not the chain you are connected to).
//
// An EIP-712 signature carries the chainId it is bound to in its domain. When
// that chainId differs from the chain the wallet is currently connected to, the
// signature is being requested for a DIFFERENT chain — a cross-chain replay /
// confusion vector (sign once "for Polygon", get it replayed on mainnet).
//
// The verdict is `domain.chainId != connected_chain`. The action carries
// `meta.nature.domain.chainId` (a Long), but the "connected chain" is the
// evaluation context's chain, which is NOT a field of the action context — only
// the fact host holds both sides. So the comparison is made off-Cedar
// (Aggregator type 9 — host-side context): the manifest's
// `signature.chain_mismatch` policy_rpc call projects the verdict into
// `context.custom.chainMismatch`.
//
// `custom` is optional, so the guard `has`-checks the path; the value is a `Bool`.
// The eval-chain comparison runs in the sim-server fact host, so the call is
// `optional` and the policy fail-opens until it ships. Scoped to
// `Token::Action::"Erc20Permit"` — an off-chain signature action that carries an
// EIP-712 domain.
@id("signature-chain-mismatch-permit-warn")
@severity("warn")
@reason("This signature is bound to a different chain than the one you are connected to")
forbid(principal, action == Token::Action::"Erc20Permit", resource)
when {
  context has custom
  && context.custom has chainMismatch
  && context.custom.chainMismatch
};
