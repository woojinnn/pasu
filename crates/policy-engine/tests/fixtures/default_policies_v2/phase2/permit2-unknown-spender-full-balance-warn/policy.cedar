// scopeball policy — Permit/Permit2 off-chain signature granting a full-balance
// (or unlimited) allowance to a spender the wallet does not recognise.
//
// 2025 telemetry has Permit/Permit2 phishing as the dominant high-value drain
// vector: a single off-chain signature whose `spender` is the attacker and whose
// `amount` covers the whole balance drains the token wholesale (PEPE $1.39M;
// spWETH $32.43M single-signature loss — decrypt, dextools, metamask, slowmist,
// gate). The existing `permit-allowance-horizon-warn` keys on absolute allowance
// USD / far-future expiry and `unlimited-approval-deny` keys on on-chain approve;
// neither folds the spender-familiarity signal nor a balance-relative share. This
// policy combines BOTH: near-full balance reach AND an unrecognised spender.
//
// Two enriched facts drive this (one method, record output):
//   - `permitFullBalancePct` — `amount / token_balance × 100`. `amount` is a raw
//                              U256 hex String (NOT Cedar-comparable) and the
//                              ratio needs the wallet's CURRENT holding of the
//                              token, so it is computed off-Cedar (precedent:
//                              `token.outflow_pct_of_holding`).
//   - `spenderIsKnown`       — Bool: is `spender` an address the wallet has
//                              previously approved/interacted with (known-good)?
//                              The interaction history lives in wallet state, so
//                              the host computes it.
//
// `custom` is optional, so each field is `has`-guarded. `permitFullBalancePct`
// is a Cedar `decimal` (use `.greaterThan(...)`, 4dp dotted); `spenderIsKnown`
// is a Bool (warn when it is false). The state reads live in the sim-server fact
// host, so the call is `optional` and the policy fail-opens until it ships.
// Threshold is user configuration; for this slice it is inlined (90 ⇒ warn when
// the permit reaches over 90% of the holding to an unknown spender).
@id("permit2-unknown-spender-full-balance-warn")
@severity("warn")
@reason("This permit signature lets an unfamiliar spender take almost your entire balance of this token")
forbid(principal, action == Token::Action::"Erc20Permit", resource)
when {
  context has custom
  && context.custom has spenderIsKnown
  && context.custom.spenderIsKnown == false
  && context.custom has permitFullBalancePct
  && context.custom.permitFullBalancePct.greaterThan(decimal("90.0000"))
};
