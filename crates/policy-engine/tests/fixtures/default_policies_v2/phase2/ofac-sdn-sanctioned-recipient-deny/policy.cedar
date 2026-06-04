// ScopeBall US compliance pack — OFAC SDN sanctions screen (pre-sign).
//
// REGIME: OFAC (Office of Foreign Assets Control, U.S. Treasury) economic
// sanctions. US persons are PROHIBITED from engaging in any transaction with a
// person or property on the Specially Designated Nationals and Blocked Persons
// (SDN) List. Since Nov 2018, OFAC publishes the SDN entry's digital-currency
// wallet addresses (Digital Currency Address — XBT/ETH/etc.) directly in the SDN
// List. Liability is STRICT: a US person can be penalized even with no knowledge
// the counterparty was sanctioned. (Authority: IEEPA, 50 U.S.C. 1701-1706;
// program regs e.g. 31 CFR Chapter V; penalty 50 U.S.C. 1705.)
//
// This is RegTech a generic simulator/blocklist cannot do: it is a JURISDICTION
// pack the US trader installs, and it enforces the U.S. SDN prohibition at the
// sign gate of their self-custody wallet — blocking an ERC20 transfer to a
// sanctioned address BEFORE the signature exists, so no prohibited transaction
// is ever broadcast.
//
// MECHANISM: extends the shipped `transfer-blocklisted-recipient-deny` pattern.
// The recipient is screened against the OFAC SDN digital-currency-address feed
// via the `external.address_blocklisted` policy_rpc call, parametrized to the
// `ofac_sdn` list (the US jurisdiction registry). The verdict projects into
// `context.custom.ofacSdnListed`. Until the feed ships the call is `optional`
// (fail-open: absence ⇒ `context.custom has ofacSdnListed` is false ⇒ no-op,
// never SystemFail). It activates automatically once the provider lands.
//
// ScopeBall assists compliance; it is not legal advice.
@id("ofac-sdn-sanctioned-recipient-deny")
@severity("deny")
@reason("Recipient is on the U.S. OFAC SDN sanctions list; transacting with it is prohibited (strict liability)")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has ofacSdnListed
  && context.custom.ofacSdnListed
};
