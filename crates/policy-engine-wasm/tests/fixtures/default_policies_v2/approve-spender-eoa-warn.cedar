// dambi default v2 policy — explorer id: GEN-14 (approving an EOA instead of
// a contract).
//
// A legitimate `approve` almost always targets a contract (a router, a vault, a
// marketplace). Granting an allowance to a plain wallet (EOA) is a strong
// phishing tell: there is no contract logic to spend through, so the grant only
// makes sense as a drainer staging step. `is_eoa` is `codehash(spender) ==
// empty` — a live on-chain `eth_getCode` view, NOT wallet state (Aggregator type
// 9 — External / live view). The manifest's `external.spender_is_eoa` policy_rpc
// call projects the verdict into `context.custom.spenderIsEoa`.
//
// `custom` is optional, so the guard `has`-checks the path; the value is a `Bool`.
// The `eth_getCode` view runs in the sim-server fact host, so the call is
// `optional` and the policy fail-opens until it ships.
@id("approve-spender-eoa-warn")
@severity("warn")
@reason("You are approving a plain wallet (EOA), not a contract — a common phishing pattern")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has spenderIsEoa
  && context.custom.spenderIsEoa
};
