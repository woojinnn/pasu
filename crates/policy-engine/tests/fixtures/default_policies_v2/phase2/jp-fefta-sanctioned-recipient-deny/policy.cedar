// BLOCKED-BY-ACTION: draft-promotion bundle; method/schema not yet catalog-ready. Skipped until banner removed.
// ScopeBall JP compliance pack — Foreign Exchange and Foreign Trade Act
// (外国為替及び外国貿易法 / FEFTA) asset-freeze sanctions.
//
// REGIME: Japan implements UN/autonomous asset-freeze sanctions under FEFTA.
// The Ministry of Finance (財務省) publishes the "経済制裁措置及び対象者リスト"
// (Economic Sanctions Measures and List of Targeted Persons). In 2022 FEFTA was
// amended so that transactions involving crypto-assets fall within the scope of
// financial sanctions; a payment or capital transaction with a designated party
// requires a licence (effectively prohibited). A self-custody trader sending to
// a sanctioned address would breach FEFTA.
//
// WHAT THIS DOES: deny an ERC20 transfer whose `recipient` is on a jurisdiction
// sanctions feed. The feed is the union of (a) the Japan MOF FEFTA asset-freeze
// list mapped to on-chain addresses and (b) the OFAC SDN crypto addresses Japan
// substantively mirrors. The manifest's `external.address_blocklisted` policy_rpc
// call (extended with a `list_source` param selecting the JP/OFAC feed) projects
// the verdict into `context.custom.recipientSanctioned`.
//
// FAIL-OPEN: the sanctions registry is an external policy-rpc host method. Until
// it ships the call is `optional`, so its absence fail-opens — `custom has
// recipientSanctioned` is false and the policy no-ops rather than SystemFail. It
// activates automatically once the provider lands. This is COMPLIANCE ASSIST,
// not legal advice; the authoritative list is the MOF/OFAC publication.
@id("jp-fefta-sanctioned-recipient-deny")
@severity("deny")
@reason("This recipient is on Japan's MOF / OFAC asset-freeze sanctions list — sending would breach the Foreign Exchange and Foreign Trade Act")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientSanctioned
  && context.custom.recipientSanctioned
};
