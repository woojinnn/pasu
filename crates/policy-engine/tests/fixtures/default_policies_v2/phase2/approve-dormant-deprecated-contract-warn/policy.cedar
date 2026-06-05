// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// BLOCKED-BY-STATE: needs a per-owner APPROVAL-INVENTORY state surface plus a
// per-spender LIVENESS/DEPRECATION time series, neither of which exists on the
// live path today. The "표면화" half of this policy (enumerate every living
// ERC20 allowance the wallet still holds toward dormant/deprecated contracts so
// the user can revoke) is not a per-request judgment at all — it ranges over the
// wallet's whole approval inventory, which policy-engine (stateless, single
// request) does not carry. The "신규 approve 경고" half below judges only the
// NEW approve's spender, and even that Bool ("is this spender dormant or an
// officially deprecated/migrated version?") must be folded from a per-spender
// last-activity + deprecation series that no state layer accumulates yet. Verify
// will demote this to needs-new-state; the files here are the TARGET shape for
// when that inventory + liveness surface lands.
//
// scopeball default v2 policy — living approvals to dead/deprecated contracts.
// Approval-hygiene family (Approval Hygiene Set).
//
// Rule: warn when the wallet grants a NEW ERC20 allowance to a spender contract
// that is dormant (no on-chain activity for a long window) or officially
// deprecated/migrated. A still-live allowance to such a contract becomes a
// drain conduit the moment that contract is compromised or a vuln is found —
// the documented Dolomite ($1.8M), 1inch TrustedVolumes (85-tx drain), BSCex
// ($8.2M) and Ekubo ($1.4M) pattern.
//
// WHY new state is required:
//   "is this spender dormant / deprecated?" is not a field of this approve
//   context (Erc20ApproveContext carries token / spender / amount / meta only).
//   The manifest declares `approval.spender_dormant_deprecated`, projecting the
//   verdict into the `Bool` `spenderDormantDeprecated`. That verdict can only be
//   trusted once a per-spender activity/deprecation series exists (it folds a
//   last-activity timestamp + a deprecation flag), which in turn presumes the
//   approval-inventory state that drives the surfacing half. Defined-not-wired →
//   `optional` (fail-open), so the field is `has`-guarded; absent ⇒ no-op.
@id("approve-dormant-deprecated-contract-warn")
@severity("warn")
@reason("This spender contract looks dormant or deprecated — granting it a live approval leaves a drain path if it is ever compromised")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has spenderDormantDeprecated
  && context.custom.spenderDormantDeprecated
};
