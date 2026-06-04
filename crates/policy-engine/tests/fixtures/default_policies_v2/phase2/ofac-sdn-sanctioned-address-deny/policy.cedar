// ScopeBall US individual pack — OFAC SDN sanctioned-address block (pre-sign).
//
// CUSTOMER = the individual US-person self-custody holder. binds_individual =
// DIRECT-INDIVIDUAL-DUTY.
//
// REGIME: OFAC (Office of Foreign Assets Control, U.S. Treasury) economic
// sanctions, administered under the International Emergency Economic Powers Act
// (IEEPA, 50 U.S.C. 1701-1706; penalties 50 U.S.C. 1705). The prohibition binds
// ALL "U.S. persons" — every US citizen/resident individual — directly. It is
// NOT a VASP-only duty: there is no exchange/intermediary in the loop on a
// self-custody P2P/DeFi transfer, yet the individual is still the "U.S. person"
// who "must ensure that they block the property and interests in property of
// persons named on OFAC's SDN List ... and that they do not engage in trade or
// other transactions with such persons" (OFAC FAQ 562). Liability is STRICT:
// civil penalty applies "without regard for whether a person has knowledge or
// reason to know" the counterparty was sanctioned. Since Nov 2018 OFAC publishes
// the SDN entry's on-chain "Digital Currency Address" (XBT/ETH/etc.) directly in
// the SDN List, so an ETH/ERC20 recipient address is a screenable SDN field.
//
// FOREIGN-REACH HONESTY: for a US-person individual this is the OWN-country
// binding law (not extraterritorial). It reaches a non-US holder only via a
// US-nexus; this pack is installed by US residents, so it is a primary duty.
//
// WHY ScopeBall (not a generic sim): this is a JURISDICTION pack the US trader
// installs. It enforces the U.S. SDN prohibition at the SIGN GATE of the
// self-custody wallet — blocking the ERC20 transfer to a sanctioned address
// BEFORE the signature exists, so no prohibited transaction is ever broadcast.
//
// MECHANISM (extends shipped transfer-blocklisted-recipient-deny): the recipient
// is screened against the OFAC SDN digital-currency-address feed via the
// `external.address_blocklisted` policy_rpc call, parametrized to the `ofac_sdn`
// list (the US jurisdiction registry). The verdict projects into
// `context.custom.ofacSdnListed`. Until the feed ships the call is `optional`
// (fail-open: absence => `context.custom has ofacSdnListed` is false => no-op,
// never SystemFail). It activates automatically once the provider lands.
//
// ScopeBall assists compliance; it is not legal advice.
@id("ofac-sdn-sanctioned-address-deny")
@severity("deny")
@reason("Recipient is on the U.S. OFAC SDN sanctions list; as a U.S. person, transacting with it is prohibited (strict liability)")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has ofacSdnListed
  && context.custom.ofacSdnListed
};
