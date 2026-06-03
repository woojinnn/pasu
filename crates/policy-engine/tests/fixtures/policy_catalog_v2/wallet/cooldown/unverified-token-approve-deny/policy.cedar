// blocks: an ERC-20 approval on a token whose contract is not verified (no published / vetted source).
// why:    Chainalysis-style unverified-contract block — approving an unverified token contract grants a standing pull on code nobody can audit; counterfeit / malicious tokens routinely ship unverified to hide a hostile transferFrom or fee hook, so an approval there is denied (unverified-token threat class).
// bucket: wallet/cooldown
// tags: bucket:wallet cat:cooldown enrichment method:token.metadata action-family:cooldown
// methods: token.metadata -> ../../../_methods/token.metadata.md
// dormant: until token.metadata is registered in method-catalog.json
//
// Cooldown — DENY an approval on a token whose contract is unverified.
// `token.metadata` enriches the verified flag. ENRICHMENT — dormant.
@id("unverified-token-approve-deny")
@severity("deny")
@reason("Token contract is unverified — approval blocked; only approve audited, verified tokens")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context has custom
  && context.custom has isVerified
  && context.custom.isVerified == false
};
