// scopeball EU compliance pack (INDIVIDUAL) — EU consolidated sanctions list:
// transfer to a frozen / listed person.
//
// LEGAL BASIS: Council Regulation (EU) No 269/2014 of 17 March 2014 concerning
// restrictive measures in respect of actions undermining or threatening the
// territorial integrity, sovereignty and independence of Ukraine.
//   Art. 2(1): "All funds and economic resources belonging to, owned, held or
//   controlled by any natural persons or natural or legal persons, entities or
//   bodies associated with them as listed in Annex I shall be frozen."
//   Art. 2(2): "No funds or economic resources shall be made available, directly
//   or indirectly, to or for the benefit of natural persons or natural or legal
//   persons, entities or bodies associated with them listed in Annex I."
// Authority: Council of the European Union. The European Commission's own
// guidance confirms crypto-assets are "funds/economic resources" covered by the
// asset-freeze and the prohibition to make funds available. The EU FSF
// (Financial Sanctions Files / EU Consolidated List) is the canonical register.
//
// BINDS_INDIVIDUAL = DIRECT-INDIVIDUAL-DUTY. Art. 2(2) binds "no person" — it is
// addressed to EVERY natural and legal person in the Union, NOT only to VASPs.
// A self-custody EU resident who sends ERC-20 value (directly OR indirectly) to
// an address owned/controlled by a listed person commits a sanctions breach in
// their OWN name. This is a peer-to-peer on-chain transfer the individual signs
// personally; no intermediary is needed for the prohibition to attach. Sanctions
// breaches are criminal offences in EU Member States (Directive (EU) 2024/1226
// harmonised criminal penalties). This is a personal legal duty, not VASP duty.
//
// JURISDICTION: residency is NOT on-chain. The EU pack is the install-time
// selector — an EU resident installs this bundle; the sanctions register is the
// EU Consolidated List, not a trigger.where field. (A non-EU person is reached
// by EU measures only on an EU nexus; here the user opts into EU law by residency.)
//
// WHAT THIS DOES: on an ERC-20 transfer whose `recipient` is on the EU
// consolidated sanctions list, DENY before signing. The host method
// external.address_blocklisted (list = "eu_consolidated_sanctions") projects the
// hit into context.custom.recipientEuSanctioned.
//
// EXPRESSIBILITY: needs-external (EU Consolidated Sanctions List registry). The
// call is `optional` (fail-open) — if the register is unreachable the field is
// absent and the policy no-ops rather than SystemFail. `custom` is optional, so
// the field is `has`-guarded before reading (has-guard triad). DENY fires only on
// a POSITIVE hit, never on absence.
//
// ENFORCEMENT EXAMPLE: OFAC/EU-coordinated designations have listed specific
// crypto wallet addresses (e.g. the EU and US designations tied to Russian
// crypto-laundering networks); sending to such an address is a freeze breach.
@id("eu-sanctions-listed-recipient-deny")
@severity("deny")
@reason("This recipient is on the EU consolidated sanctions list (Reg (EU) 269/2014 Art 2) — making funds available to a listed person is prohibited for every EU person")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientEuSanctioned
  && context.custom.recipientEuSanctioned
};
