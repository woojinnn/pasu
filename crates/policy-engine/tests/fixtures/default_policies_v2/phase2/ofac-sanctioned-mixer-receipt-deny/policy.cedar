// ScopeBall US individual pack — OFAC-sanctioned CVC-mixer block (pre-sign).
//
// CUSTOMER = the individual US-person self-custody holder. binds_individual =
// DIRECT-INDIVIDUAL-DUTY.
//
// REGIME: OFAC designation of virtual-currency ("CVC") mixers as SDNs. OFAC's
// first mixer designation was Blender.io (May 6, 2022), followed by Tornado Cash
// (Aug 8, 2022, E.O. 13694) and Sinbad.io (Nov 29, 2023). On designation, "all
// property and interests in property of the entity ... that is in the United
// States or in the possession or control of U.S. persons is blocked and must be
// reported to OFAC," and "all transactions by U.S. persons ... that involve any
// property or interests in property of designated or otherwise blocked persons
// are prohibited" (Treasury press release, Tornado Cash designation). A US-person
// individual who deposits funds into — or withdraws/receives from — a sanctioned
// mixer's blocked address is engaging in a prohibited transaction with blocked
// property; the duty is STRICT-LIABILITY and binds the individual directly, with
// no VASP intermediary required.
//
// HONESTY / CURRENCY OF DATA: Tornado Cash was DELISTED by OFAC on Mar 21, 2025
// following Van Loon v. Treasury (5th Cir., Nov 26, 2024 — immutable smart
// contracts held not "property" under IEEPA), so the Tornado Cash addresses are
// NO LONGER SDN-listed and must NOT be hard-blocked on sanctions grounds. The
// LIVE, currently-sanctioned mixers are Blender.io and Sinbad.io (designations
// not disturbed by the Tornado Cash ruling). This policy therefore screens
// against the CURRENT `ofac_cvc_mixer` SDN sublist (the registry owner removes
// delisted entries), not a hard-coded address — so it tracks designations as
// they change. The duty is the individual's; the registry keeps it accurate.
//
// MECHANISM: the outgoing recipient (a deposit address into a mixer) is screened
// against the OFAC CVC-mixer SDN sublist via `external.address_blocklisted`,
// list = `ofac_cvc_mixer`. Verdict projects into `context.custom.ofacMixerListed`.
// Inbound receipts from a mixer are not signed by this wallet, so the
// pre-sign-enforceable surface is the OUTGOING deposit leg; this catches the
// individual funding a sanctioned mixer. Call is `optional` (fail-open until the
// feed ships); `custom` is optional and `has`-guarded.
//
// ScopeBall assists compliance; it is not legal advice.
@id("ofac-sanctioned-mixer-receipt-deny")
@severity("deny")
@reason("Recipient is an OFAC-sanctioned virtual-currency mixer (e.g. Blender.io / Sinbad.io); as a U.S. person, transacting with it is prohibited")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has ofacMixerListed
  && context.custom.ofacMixerListed
};
