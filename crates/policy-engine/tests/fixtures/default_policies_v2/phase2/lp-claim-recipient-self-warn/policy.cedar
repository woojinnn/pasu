// scopeball default v2 policy — explorer id: LP-05 (claim/refund recipient is
// an address other than me).
//
// ACTION-FIELD comparison. A phishing UI quietly swaps the "recipient" of a
// claim/refund to the attacker. A legitimate self-claim almost always pays the
// connected wallet, so a recipient that is not me warrants a warning.
//
// sv = COV: no durable holding state is needed — both operands live in the
// lowered action context. `context.recipient` (a non-optional base `String` of
// `ClaimAllocationContext`) is compared against `context.meta.submitter` — the
// `Core::ActionMeta.submitter`, i.e. the tx sender / connected wallet (`from`),
// which becomes the `Wallet` principal. Both are non-optional `String`, so the
// comparison reads directly with no method and no `custom_context`. Warn when
// the recipient is NOT the submitter. (Variants with no recipient field are
// moot — they lower to a different action and never match this trigger.)
@id("lp-claim-recipient-self-warn")
@severity("warn")
@reason("This claim pays an address that is not your connected wallet")
forbid(principal, action == Launchpad::Action::"ClaimAllocation", resource)
when {
  context.recipient != context.meta.submitter
};
