// scopeball KR compliance pack (INDIVIDUAL) — 금융거래등제한대상자(designated
// person) / UN-sanctioned address: transfer to a frozen / designated party.
//
// LEGAL BASIS:
//   공중 등 협박목적 및 대량살상무기확산을 위한 자금조달행위의 금지에 관한 법률
//   (Act on Prohibition against the Financing of Terrorism and Proliferation of
//   Weapons of Mass Destruction; "테러자금금지법" / "공중협박자금조달금지법").
//     제4조(금융거래등제한대상자 지정 등): 금융위원회(Financial Services
//       Commission) designates persons related to terror/WMD financing as
//       "금융거래등제한대상자". A designated person — AND, by 제4조제4항, their
//       counterparty — must obtain prior FSC permission ("허가") to conduct a
//       financial transaction or dispose of property to/with the designated party.
//     제6조제2항: "제4조제4항에 따른 허가를 받지 아니하고 … 한 금융거래등제한대상자
//       또는 그 상대방 … 은 3년 이하의 징역 또는 3천만원 이하의 벌금에 처한다."
//       (A designated person OR THEIR COUNTERPARTY who transacts without FSC
//       permission: up to 3 years' imprisonment or a fine up to KRW 30 million.)
//   This is reinforced by the 외국환거래법 (Foreign Exchange Transactions Act,
//   제15조의2 지급·수령 허가) and the UN Security Council sanctions lists that
//   Korea implements (KoFIU is the competent FIU). KoFIU publishes the domestic
//   designation register.
//   Authority: 금융위원회(FSC) / 금융정보분석원(KoFIU).
//
// BINDS_INDIVIDUAL = DIRECT-INDIVIDUAL-DUTY. 제6조제2항 expressly criminalises
// "그 상대방" — the COUNTERPARTY — of the designated person, not only VASPs and
// not only the designated person. A self-custody Korean resident who signs an
// on-chain ERC-20 transfer to an address owned/controlled by a designated person,
// without prior FSC 허가, commits the offence IN THEIR OWN NAME. The prohibition
// attaches to the peer-to-peer transfer the individual personally signs; no
// intermediary is required. This is a personal criminal duty, not a VASP duty.
//
// JURISDICTION: residency / nationality is NOT on-chain — it is the install-time
// PACK SELECTOR. A Korean resident installs the KR bundle; the designation
// register (KoFIU domestic list + UN consolidated list as implemented by Korea)
// is read from an external registry, NOT encoded in trigger.where.
//
// WHAT THIS DOES: on an ERC-20 transfer whose `recipient` is on the KoFIU
// designated / UN-sanctioned register, DENY before signing. The host method
// external.address_blocklisted (list = "kr_kofiu_designated_and_un_sanctions")
// projects the hit into context.custom.recipientKrDesignated.
//
// EXPRESSIBILITY: needs-external (KoFIU designation register + UN sanctions
// registry). The call is `optional` (fail-open) — if the register is unreachable
// the field is absent and the policy no-ops rather than SystemFail. `custom` is
// optional, so the field is `has`-guarded before reading (has-guard triad). DENY
// fires only on a POSITIVE hit, never on absence.
//
// ENFORCEMENT EXAMPLE: Korea has implemented UN/OFAC-coordinated designations of
// DPRK-linked actors (e.g. the Lazarus Group wallet addresses tied to the 2022
// Ronin/Harmony bridge thefts), which OFAC and partner jurisdictions have listed
// as specific crypto addresses; a Korean person sending value to such an address
// without FSC permission breaches 테러자금금지법 제6조제2항.
@id("kr-terror-financing-designated-recipient-deny")
@severity("deny")
@reason("This recipient is on the KoFIU designated / UN-sanctioned register (테러자금금지법 제4·6조) — transferring to a designated party without FSC permission is a criminal offence that binds you personally")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has recipientKrDesignated
  && context.custom.recipientKrDesignated
};
