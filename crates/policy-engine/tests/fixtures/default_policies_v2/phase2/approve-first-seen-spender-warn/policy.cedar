// scopeball draft policy — Approval Hygiene Set / approve-first-seen-spender-warn.
// Twitter signal 'unlimited-token-approval-drain': "wallets are not hacked, they
// are approved." Users want a beat of friction when granting an approval to a
// contract they have NEVER interacted with before — the burner-spender pattern
// every drainer campaign reuses.
//
// Orthogonal to the three existing approval gates:
//   * unlimited-approval-deny     — keys on the AMOUNT (unlimited / over-balance).
//   * approve-spender-eoa-warn    — keys on the spender being an EOA (codehash).
//   * approve-fresh-domain-...    — keys on the request ORIGIN domain (external).
// This one keys ONLY on the wallet's own interaction HISTORY with the spender:
// has this owner ever granted this spender an allowance / called it before? If
// not, it is "first-seen" and we warn — even when the amount is bounded, the
// spender is a contract, and the domain is established.
//
// `spenderIsFirstSeen` is the enriched Bool projected by the manifest's
// `approval.spender_first_seen` policy_rpc call. This is an indexable on-chain
// Approval-event / tx-history read (NOT new wallet state): the host can answer
// "did (owner, spender) ever have a nonzero allowance / direct interaction?"
// from chain history. The call is `optional` (fail-open) until the fact host
// ships it, so `custom` and the field are both `has`-guarded; unknown ⇒ false ⇒
// never fires. A prior interaction ⇒ false ⇒ re-approving your usual router does
// not trip this.
@id("approve-first-seen-spender-warn")
@severity("warn")
@reason("You have never interacted with this spender before — approving a first-seen contract is the most common drainer staging step")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has spenderIsFirstSeen
  && context.custom.spenderIsFirstSeen
};
