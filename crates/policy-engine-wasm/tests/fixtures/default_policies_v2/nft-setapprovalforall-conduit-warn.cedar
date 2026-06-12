// dambi default v2 policy — explorer id: NFT-04 (setApprovalForAll to an
// off-allowlist conduit/operator; the NFT-marketplace instance of GEN-02).
//
// `setApprovalForAll(operator, true)` is the on-chain precondition that makes a
// zero-price-listing drain *executable*: it hands one operator (a Seaport
// conduit, a Blur exchange, …) the right to move every token in the collection.
// Phishing "free mint" sites always request this first. Warn when collection-
// wide operator rights are granted (`approved == true`) to a `spender` that is
// not a known marketplace conduit.
//
// `spender` and `approved` are base fields of `NftSetApprovalForAllContext`, so
// no enrichment is needed (Aggregator type 5 — Set Membership against a
// literal set). The marketplace-conduit allowlist is user/marketplace
// configuration; for this slice it is inlined as a Cedar set literal
// (personalized supply via a Set<String> custom field is a later increment —
// design proposal F#2). Addresses are lowercase hex; sample entries are the
// OpenSea Seaport 1.1 conduit and the Blur ExecutionDelegate.
@id("nft-setapprovalforall-conduit-warn")
@severity("warn")
@reason("setApprovalForAll grants this operator your entire collection, and it is not a recognized marketplace conduit")
forbid(principal, action == Token::Action::"NftSetApprovalForAll", resource)
when {
  context.approved
  && !([
    "0x1e0049783f008a0085193e00003d00cd54003c71",
    "0x00000000000111abe46ff893f3b2fdf1f759a8a8"
  ].contains(context.spender))
};
