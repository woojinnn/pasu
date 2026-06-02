// scopeball default v2 policy — explorer id: AMMLP-1 (remove_liquidity output
// redirected to an address other than the signer).
//
// Phishing UIs swap one field — `recipient` — for the attacker on LP exit, a
// flow the swap-output guard (UNI-01) does not cover. `recipient` lives at
// `params.recipient` for the `pooled_burn` variant (an OPTIONAL nested String;
// the V3 variants send to the NFT owner and carry no recipient field), so this
// `has`-guards the nested path then compares it to `principal.address`. Both are
// lowercase hex Strings — exact equality holds — with NO enrichment (sv = COV).
@id("ammlp-remove-recipient-not-self-deny")
@severity("deny")
@reason("This liquidity withdrawal sends the funds to an address that is not your wallet")
forbid(principal, action == Amm::Action::"RemoveLiquidity", resource)
when {
  context.params has recipient
  && context.params.recipient != principal.address
};
