// scopeball default v2 policy — explorer id: GEN-02 (setApprovalForAll to a
// non-allowlisted operator).
//
// `setApprovalForAll(operator, true)` hands an operator the right to move every
// token in a collection with a single signature — the standard NFT-drain
// primitive. Warn when collection-wide operator rights are granted
// (`approved == true`) to a `spender` outside the user's operator allowlist.
//
// `spender` and `approved` are base fields of `NftSetApprovalForAllContext`, so
// no enrichment is required (Aggregator type 5 — Set Membership, evaluated
// against a literal set). The allowlist is user configuration; for this slice
// it is inlined as a Cedar set literal (design proposal F#2 — personalized
// supply via enrichment/Set<String> custom field is a later increment). Sample
// entry below is the OpenSea Seaport conduit; addresses are lowercase hex.
@id("setapprovalforall-operator-warning")
@severity("warn")
@reason("setApprovalForAll grants this operator your entire collection — it is not in your allowlist")
forbid(principal, action == Token::Action::"NftSetApprovalForAll", resource)
when {
  context.approved
  && !(["0x1e0049783f008a0085193e00003d00cd54003c71"].contains(context.spender))
};
