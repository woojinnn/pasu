// blocks: staking native ETH into a liquid-staking venue above the $50k USD cap.
// why:    deposit-sizing risk — a large liquid-stake concentrates funds in one LST
//         protocol's slashing/depeg/withdrawal-queue surface; a USD ceiling forces a
//         size confirmation. source: Fireblocks transaction-authorization USD limits.
// bucket: wallet/usd-cap
// tags: bucket:wallet cat:usd-cap enrichment method:oracle.usd_value action-family:usd-cap
// methods: oracle.usd_value -> ../../../_methods/oracle.usd_value.md
//
// VOCAB NOTE: LiquidStaking::Stake carries `amount` (native ETH msg.value) and no
// `asset` field — the staked asset is native ETH. The manifest binds the `asset`
// param selector to `$.action.venue` so the /v1/rpc planner can derive the native
// asset for the venue's chain; the policy below reads only the projected USD leaf,
// so it compiles regardless of how the planner resolves the asset.
// `oracle.usd_value` enriches the staked native amount into `context.custom.stakeUsd`.
// ENRICHMENT — dormant until the /v1/rpc dispatcher serves `oracle.usd_value`.
@id("stake-usd-cap")
@severity("warn")
@reason("Large stake (over $50k) — confirm the amount before signing")
forbid(principal, action == LiquidStaking::Action::"Stake", resource)
when {
  context has custom
  && context.custom has stakeUsd
  && context.custom.stakeUsd.greaterThan(decimal("50000.0000"))
};
