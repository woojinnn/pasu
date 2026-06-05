// scopeball default v2 policy — explorer id: GOV-02 (re-delegating a LARGE block
// of voting power to a NEW address).
//
// METHOD-BACKED magnitude + DIRECT field re-delegation check (sv = PART). The
// larger the voting power, the bigger the target; phishing that re-points that
// power to a fresh address does outsized damage. Two signals combine:
//   1. SIZE — `votingPower` arrives as a U256 HEX STRING (`DelegateContext`),
//      which Cedar CANNOT compare numerically. The new method
//      `governance.voting_power_at_risk` reads the durable
//      `StakeReceipt.voting_power` (or, absent state, the live `votingPower`)
//      and emits the comparable `decimal` `votingPowerAtRisk`. Defined-not-wired
//      → `optional`, fail-open until sim-server serves it.
//   2. RE-DELEGATION — warn only when the delegatee differs from the current
//      delegate. `delegatee` (base String) and `currentDelegate?` (live String)
//      are both plain strings, so Cedar tests inequality DIRECTLY. The current
//      delegate is an Action live_input, NOT durable State (no on-chain delegate
//      surface in WalletState) → re-delegation detection is PARTIAL.
// "내 현재 delegate"가 durable State에 없어 live_inputs로만 보완(PARTIAL).
// Threshold is user config (`delegate_power_warn`).
@id("gov-redelegate-large-power-warn")
@severity("warn")
@reason("This re-delegates a large block of voting power to a new address")
forbid(principal, action == Airdrop::Action::"Delegate", resource)
when {
  context has custom
  && context.custom has votingPowerAtRisk
  && context.custom.votingPowerAtRisk.greaterThan(decimal("100000.0000"))
  && context has currentDelegate
  && context.delegatee != context.currentDelegate
};
