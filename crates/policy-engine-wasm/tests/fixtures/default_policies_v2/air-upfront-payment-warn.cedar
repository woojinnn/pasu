// scopeball default v2 policy — explorer id: AIR-06 (claim demands an upfront
// approve / fee / "unlock" payment first — the classic advance-fee scam).
//
// METHOD-BACKED bundle fold (Aggregator type 9; sv = COV). A real claim only
// receives; an approve or a native (msg.value > 0) transfer ORDERED BEFORE the
// claim inside the bundle is the scam signal. The single lowered
// `Airdrop::ClaimContext` cannot see its sibling sub-actions, so the detection
// is a fold over the multicall children preceding the claim — a NEW reducer
// fact `claim.upfront_payment_required`, projected into
// `context.custom.requiresUpfrontPayment` (Bool). Defined-not-wired →
// `optional`, so its absence fail-opens until the fact host serves it. Warn when
// a preceding payment/approval is present.
@id("air-upfront-payment-warn")
@severity("warn")
@reason("This claim asks you to approve or pay before receiving — a hallmark of advance-fee airdrop scams")
forbid(principal, action == Airdrop::Action::"Claim", resource)
when {
  context has custom
  && context.custom has requiresUpfrontPayment
  && context.custom.requiresUpfrontPayment
};
