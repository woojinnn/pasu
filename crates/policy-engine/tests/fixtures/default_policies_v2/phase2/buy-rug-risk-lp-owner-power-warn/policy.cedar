// scopeball policy — Token Due Diligence Set: rug-pull liquidity/ownership risk
// on a token BUY (Amm::Swap, tokenOut is the asset being acquired).
//
// Two distinct rug vectors, OR-ed: (1) the bought token's LP is unlocked or its
// supply is concentrated in the dev / a single holder — the dev can pull pool
// liquidity right after you buy and crater the price to zero; (2) the token's
// owner retains an unbounded mint (dilutes your holding to dust) or
// blacklist/pause power (freezes your ability to sell). Both are off-chain,
// non-wallet signals (locker contracts, holder distribution, source/proxy
// authority scans) — Aggregator type 9 (External Fact Lookup), NOT wallet
// state. The method `external.token_rug_risk` folds both probes and projects
// two Bools into `context.custom`.
//
// Honeypot / unsellable-token detection is a SEPARATE policy
// (swap-out-token-honeypot-warn); this one targets sellable tokens whose funds
// can be yanked or frozen AFTER the buy.
//
// The policy_rpc call is `optional`, so until the provider ships the call
// fail-opens: `context.custom has <f>` is false and the policy no-ops rather
// than SystemFail. Each Bool is `has`-guarded before reading.
@id("buy-rug-risk-lp-owner-power-warn")
@severity("warn")
@reason("This token shows rug-pull risk — its liquidity is unlocked or dev-concentrated, or its owner can mint or freeze your tokens")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && (
    (context.custom has lpUnlockedOrDevConcentrated && context.custom.lpUnlockedOrDevConcentrated)
    || (context.custom has ownerHasMintOrBlacklist && context.custom.ownerHasMintOrBlacklist)
  )
};
