// blocks: a native-ETH liquid-staking deposit going through without the user explicitly anchoring the intent (staking locks funds into a venue and a withdrawal queue).
// why:    liquid staking moves native ETH into a staking contract and a liquid-staking token whose redemption is subject to a withdrawal queue/delay — a deliberate, semi-illiquid commitment worth confirming at sign time. Source: Lido stake / withdrawal-queue docs.
// bucket: action/staking
// tags: bucket:action cat:staking static action-family:stake-confirm
// Pure-static intent anchor: fires on every stake. No `when` clause — the action
// match alone is the confirm trigger.
@id("stake-deposit-confirm")
@severity("warn")
@reason("You are staking native ETH into a liquid-staking venue — confirm; redemption is subject to a withdrawal queue")
forbid(principal, action == LiquidStaking::Action::"Stake", resource);
