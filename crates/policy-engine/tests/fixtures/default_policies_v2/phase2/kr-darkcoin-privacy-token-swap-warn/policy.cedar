// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// scopeball KR compliance pack (INDIVIDUAL) — acquiring a privacy / "dark" coin
// (Monero / Dash / Zcash class) delisted by Korean exchanges per FATF / 특금법.
//
// LEGAL BASIS (frames the harm — this is PROTECTION, not an individual duty):
//   특정 금융거래정보의 보고 및 이용 등에 관한 법률 시행령 (Enforcement Decree of
//   the FTRA / 특금법 시행령), 제10조의20(가상자산사업자의 조치), 시행 2021-03-25.
//     A 가상자산사업자(VASP) must NOT handle a virtual asset whose embedded
//     technology makes the transfer record un-identifiable on transmission —
//     i.e. anonymity / dark coins ("가상자산 이전 시 전송기록이 식별될 수 없도록
//     하는 기술이 내재된 가상자산의 취급 금지"). This is implemented to satisfy the
//     FATF Travel Rule (FATF Recommendation 16), which dark coins cannot
//     structurally comply with.
//   Authority: 금융위원회(FSC) / 금융정보분석원(KoFIU); FATF (standard-setter).
//   EFFECT: under this rule Korean exchanges (Upbit, Bithumb, Korbit, Coinone,
//     OKEx Korea) delisted Monero (XMR), Dash (DASH), Zcash (ZEC) and similar
//     anonymity coins from 2019–2021. NOTE: the prohibition binds the VASP/
//     EXCHANGE, NOT the self-custody individual.
//
// BINDS_INDIVIDUAL = INDIVIDUAL-PROTECTION. There is no Korean law forbidding an
// individual from HOLDING a privacy coin in self-custody; the 취급금지 duty falls
// on the EXCHANGE. We therefore DO NOT assert an individual legal duty. Instead
// this protects the individual from REAL, CONCRETE harm flowing directly from the
// VASP-side ban:
//   (1) NO CASH-OUT VIA KR EXCHANGES — every compliant Korean exchange has
//       delisted these assets, so the user cannot deposit/sell them for KRW;
//       liquidity to fiat is effectively closed onshore;
//   (2) AML-FLAG / banking-friction exposure — moving privacy-coin proceeds back
//       into the regulated system attracts enhanced AML scrutiny;
//   (3) liquidity / counterparty risk on the remaining (often unregistered)
//       venues. This is loss-/friction-avoidance for the individual, not a duty.
//
// SEVERITY = warn (not deny): holding/acquiring a privacy coin is lawful for the
// individual; the user may knowingly accept the cash-out friction. The policy
// surfaces the onshore-illiquidity red flag and asks the user to confirm.
//
// JURISDICTION: residency is NOT on-chain; the KR pack is the install-time
// selector. The anonymity-coin / KR-delisted classification is read from a
// KR-aligned registry, NOT from trigger.where.
//
// WHAT THIS DOES: on an AMM swap whose `tokenOut` is classified as an anonymity /
// dark coin OR is marked delisted by Korean exchanges, WARN before signing. The
// host method external.kr_darkcoin_classification projects two flags into
// context.custom.
//
// EXPRESSIBILITY: needs-external (KR dark-coin / delisted-asset classification
// registry). The call is `optional` (fail-open); `custom` is optional, so each
// flag is `has`-guarded before reading (has-guard triad). WARN fires only on a
// POSITIVE flag.
//
// ENFORCEMENT EXAMPLE: in 2021 Korean exchanges (Upbit, Bithumb, OKEx Korea)
// completed delisting of Monero/Dash/Zcash citing the 특금법 시행령 dark-coin
// rule and FATF Travel Rule; XMR/DASH/ZEC have since had no won-pair onshore.
@id("kr-darkcoin-privacy-token-swap-warn")
@severity("warn")
@reason("The token you are about to receive is a privacy / dark coin delisted by Korean exchanges under the 특금법 시행령 dark-coin rule + FATF Travel Rule — you cannot cash it out via a KR exchange and face AML-flag / liquidity risk")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && (
    (context.custom has outTokenIsDarkcoin && context.custom.outTokenIsDarkcoin)
    || (context.custom has outTokenDelistedKr && context.custom.outTokenDelistedKr)
  )
};
