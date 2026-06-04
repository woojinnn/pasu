// scopeball default v2 policy — Morpho withdrawal liquidity jam.
// Source: policy-factory cycle 2026-06-03-morpho (protocol scope = Morpho only).
// Demand (DECIDING = twitter 1st-person + CONFIRMATORY = forum/docs):
//   - X: "money stuck inside a Morpho Vault on Base ... they decommissioned it
//     ... now we can't withdraw our deposits" (x.com/i/status/2060587927747662077)
//   - X: "team currently has $40M of the backing stuck on morpho" (.../2062064379957481760)
//   - Morpho forum: withdraw "transaction fails all the time with the message
//     'Try again'" (forum.morpho.org/t/.../1859)
//   - Morpho docs: "all liquidity is borrowed ... you might not be able to
//     withdraw your assets right away" (docs.morpho.org/build/earn/.../assets-flow)
//   - 3-4 of ~320 vaults recently pinned at ~100% utilization (crypto-economy, 2025-11-06)
//
// ACTION-FIELD. A Morpho/MetaMorpho withdrawal can only draw the FREE cash in
// the market's isolated reserve. When utilization pins near 100% or the reserve
// is paused/frozen, the withdraw reverts even though the deposit is not lost —
// invisible on the wallet screen. The reserve flags/utilization ride in the
// withdraw action's live_inputs and are exposed as base context fields
// reserveState.{isPaused,isFrozen,utilizationBp} (Bool/Bool/Long). Pure read of
// base context, so the manifest carries NO policy_rpc and NO custom_context.
// Venue-scoped to Morpho via the manifest trigger (morpho_blue + morpho_optimizer).
//
// RUNTIME NOTE: the host must populate reserveState live_inputs on the Morpho
// withdraw path. Today crates/policy-server/.../effect/lending/morpho_blue.rs is
// a Phase-2 stub (reuses ReserveState as a stand-in); until that wiring lands
// this policy is gate-valid but a no-op at runtime. The .cedar + manifest are
// consistent NOW and specify exactly what the host must fill.
@id("morpho-withdraw-illiquid-or-paused-warn")
@severity("warn")
@reason("This Morpho market/vault is paused or near full utilization — your withdrawal may fail or be delayed until borrowers repay")
forbid(principal, action == Lending::Action::"Withdraw", resource)
when {
  context.reserveState.isPaused
  || context.reserveState.isFrozen
  || context.reserveState.utilizationBp > 9500
};
