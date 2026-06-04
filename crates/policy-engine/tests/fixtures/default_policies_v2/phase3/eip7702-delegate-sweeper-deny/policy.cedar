// BLOCKED-BY-ACTION: staged via policy-factory draft promotion; needs new action/state surface or external host before it can compile/conform. Skipped by all consumers until this banner is removed.
// scopeball golden-sample policy — eip7702-delegate-sweeper-deny.
//
// ====================================================================
// needs-new-action  (REQUIREMENT #2 — DECLARED PROMINENTLY)
// ====================================================================
// NO EIP-7702 action model exists in scopeball today. This package is
// authored against a PROPOSED action surface (see infra_declaration and
// reports/needs-new-action-eip7702.md). It does NOT evaluate until the
// action pipeline lands:
//
//   1. DECODE (request-router + sign-resolver): recognise a type-0x04
//      (SetCode) tx carrying a non-empty `authorization_list`, AND the
//      standalone EIP-712 / auth-tuple sign flow. Parse each tuple
//      (chainId, addr, nonce, yParity, r, s) — `addr` is the DELEGATE
//      TARGET. Wallets render only the outer tx fields and DO NOT show
//      this list (arXiv:2512.12174), so the delegate is invisible until
//      we decode it.
//   2. ACTION (asset-model/action + schema/policy-schema): new action
//      (working name) `Eip7702::Authorize`, base fields:
//        delegate     : String   (the tuple `addr`, lowercase hex)
//        authority    : String   (the EOA being delegated = principal)
//        authChainId  : Long     (the tuple `chainId`)
//        nonce        : Long
//        isSelf       : Bool     (delegate == authority → own smart acct)
//        scoped       : Bool     (delegation carries expiry / single-use
//                                 / allow-list constraints)
//      Reserve action.tag = "eip7702_authorize".
//   3. LOWERING: DecodedCall / typed-data → the action above; expose
//      action.tag so this trigger.where { action.tag eq "eip7702_authorize" }
//      fires.
//
// Until (1)-(3) ship, the trigger never matches → this policy is inert
// (no SystemFail). Once they ship, it activates automatically.
//
// ====================================================================
// THE THREAT (why a pre-sign scope gate is the ONLY layer that catches it)
// ====================================================================
// A SINGLE EIP-7702 authorization tuple silently converts the user's EOA
// into a smart account whose code is `delegate`. The signature "moves no
// funds now" — so value-diff / balance-delta detection sees nothing — but
// it hands `delegate` PERSISTENT, unconditional `execute` power to batch-
// drain the whole balance now and every future deposit. Wintermute: >97%
// of mainnet 7702 delegations point at near-identical sweeper bytecode
// ("CrimeEnjoyor"); ScamSniffer: $12M+ from 15,000+ wallets in Aug 2025.
// Explorers, antivirus, and revoke tools all act AFTER signing — and a
// 7702 delegation auto-drains forever, so "revoke later" is too late.
//
// ====================================================================
// AGGREGATOR — External Fact Lookup (type 9), needs-external
// ====================================================================
// The delegate address alone is not classifiable in Cedar. Three OPTIONAL
// policy_rpc calls (all fail-open until the fact host ships) project Bools
// into context.custom, keyed on the decoded `delegate`:
//   - external.address_blocklisted          → delegateIsKnownDrainer
//        (REUSED shipped method — Inferno Drainer / known-sweeper feed)
//   - external.delegate_bytecode_class       → delegateIsSweeperBytecode
//        (NEW — matches copy-pasted CrimeEnjoyor/sweeper bytecode even on
//         a FRESH address, catching clones the blocklist hasn't seen)
//   - external.delegate_canonical_allowlisted→ delegateIsKnownSafe
//        (NEW — allowlist of canonical delegators: MetaMask DeleGator,
//         Ambire, ZeroDev/Kernel, Safe, Biconomy)
//
// ====================================================================
// THIS FILE = the P0 hard-deny tier. Two more tiers ship as sibling
// packages once the action exists (documented here for the judges):
//   * eip7702-delegate-sweeper-deny      (THIS) — Fail: delegate is a
//       known drainer OR sweeper-bytecode and NOT canonical-safe.
//   * eip7702-unknown-target-warn        — Warn: delegate not on the
//       canonical allowlist (first-seen delegation = full takeover scope).
//   * eip7702-authorize-with-batch-warn  — Warn: delegation bundled with
//       a batch outflow (reuse multicall.outflow pattern).
// ====================================================================
//
// DENY when the decoded delegate is a known drainer, OR carries sweeper
// bytecode while NOT being a canonical/known-safe delegator. The has-guard
// triad keeps every projected field optional, so an absent fact host
// fail-opens (Bool defaults false) rather than SystemFail.
@id("eip7702-delegate-sweeper-deny")
@severity("deny")
@reason("이 서명은 자금을 옮기는 게 아니라, 당신의 지갑(EOA)을 알려진 드레이너/스위퍼 컨트랙트에 영구 위임합니다. 지금과 앞으로의 모든 잔액을 일괄 인출당할 수 있습니다 — 서명하지 마세요.")
forbid(principal, action == Eip7702::Action::"Authorize", resource)
when {
  context has custom
  && (
    (context.custom has delegateIsKnownDrainer && context.custom.delegateIsKnownDrainer)
    || (
      context.custom has delegateIsSweeperBytecode
      && context.custom.delegateIsSweeperBytecode
      && !(context.custom has delegateIsKnownSafe && context.custom.delegateIsKnownSafe)
    )
  )
};
