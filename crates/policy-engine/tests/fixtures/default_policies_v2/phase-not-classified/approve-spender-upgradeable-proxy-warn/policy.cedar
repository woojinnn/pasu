// scopeball default v2 policy — ERC-20 approve granting allowance to a spender
// that is an upgradeable proxy (EIP-1967) whose admin is NOT a timelock /
// multisig / null (i.e. unilaterally mutable).
//
// Upgradeable-proxy drain: even an audited spender can be drained later if an
// admin swaps the implementation behind the proxy — the on-chain bytecode never
// changes, only the logic it delegates to. OWASP Smart Contract Top 10 (2026)
// added proxy/upgradeability as a NEW category off 2025 breach data (Unleash
// Protocol $3.9M 2025-12-30, Arbitrum proxy exploit $1.5M 2026-01-05, UPCX ~$70M
// via ProxyAdmin upgrade). DISTINCT from approve-spender-unknown-contract-warn
// (unverified) and approve-dormant-deprecated-contract-warn (dormant): a
// verified, active contract is still risky if it is unilaterally upgradeable.
//
// `approval.spender_is_mutable_proxy` reads the EIP-1967 implementation+admin
// storage slots of the spender and returns true when upgradeable AND admin is
// mutable. Projected into context.custom.spenderMutableProxy, 3-guarded.
// ENRICHMENT (needs-method) — dormant until the dispatcher serves the method.
@id("approve-spender-upgradeable-proxy-warn")
@severity("warn")
@reason("Spender is an upgradeable proxy with a mutable admin — its implementation can be swapped after approval to drain this allowance")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has spenderMutableProxy
  && context.custom.spenderMutableProxy == true
};
