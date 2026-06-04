// BLOCKED-BY-ACTION: draft-promotion bundle; method/schema not yet catalog-ready. Skipped until banner removed.
// scopeball GOLDEN SAMPLE — honeypot-buy-block-presign
// (simulate the SELL before the user signs the BUY).
//
// ============================ THE THREAT ============================
// A honeypot token accepts your money on the BUY but the contract will not let
// you SELL it back: the sell path reverts, or carries a ~100% sell tax, or only
// owner-whitelisted addresses may sell, or the tax is owner-modifiable and gets
// flipped to 100% the moment liquidity is deep. From calldata and an explorer
// the BUY looks identical to a legit swap — a green quote and a green "Swap"
// button. The loss is irreversible the instant you sign: the money is now in a
// contract you can never exit. SQUID (2021) was exactly this — the contract
// "allowed only the creators to sell" and trapped 43,000+ buyers.
//
// ======================= WHY A PRE-SIGN GATE =======================
// Explorers, antivirus, and revoke tools all act AFTER the buy — too late.
// Standalone scanners (Honeypot.is, GoPlus, Token Sniffer) work but require the
// user to remember to copy-paste the address into a separate tab BEFORE buying —
// a step beginners skip during FOMO. ScopeBall sits on the EXACT swap signature:
// it decodes the swap calldata to learn which token you are ACQUIRING
// (`action.tokenOut`), runs a buy-then-sell round-trip simulation against live
// forked chain state, and verdicts at the last gate — automatically, no extra
// step. It catches the SCOPE truth: "this swap gives you a token you can never
// sell back," precisely when it can still stop you.
//
// ====================== INFRA REQUIREMENT (#2) =====================
// This policy needs a NEW EXTERNAL surface that does not exist in the engine
// today: a HONEYPOT SIMULATION ORACLE (a policy-rpc sidecar). It forks current
// chain state and simulates buy-then-sell on `tokenOut` through the swap router
// to MEASURE sellability + effective sell tax + revert + modifiable-tax /
// owner-blacklist flags. The equivalent capability is proven and live in GoPlus
// Token Security and Honeypot.is, so it is buildable today; ScopeBall's novelty
// is WIRING it to the pre-sign swap gate instead of a manual lookup.
//   method: honeypot.sim(chain_id, owner, token, router) -> {
//     can_buy: Bool, can_sell: Bool, sell_reverts: Bool,
//     sell_tax_ratio: Decimal (0..1), tax_modifiable: Bool,
//     owner_can_blacklist: Bool }
// The shipped fixture `swap-out-token-honeypot-warn` (UNI-05) is the COARSE
// ancestor of this policy: it WARNs on a single host-supplied `outTokenHoneypot`
// Bool from a token CLASSIFICATION registry. This golden sample UPGRADES it to a
// hard FAIL driven by an ACTIVE round-trip SELL SIMULATION, with structured
// sub-signals so the verdict can name the exact reason (sell reverts / 100% sell
// tax / owner-gated sell).
//
// ========================== EXPRESSIBILITY ==========================
// needs-external. The Cedar engine adds NO new wallet-state surface — it only
// reads a host-supplied record projected into `context.custom.honeypot`. The
// call is `optional` and `custom` is optional, so EVERY field is `has`-guarded
// before reading (has-guard triad). Until the oracle ships, the flag's absence
// fail-opens (no-op) rather than SystemFail, and the policy activates
// automatically once the provider lands.
//
// HARD FAIL (deny) — the SQUID pattern, unconditional unsellability:
//   can_sell == false  ||  sell_reverts == true  ||  sell_tax_ratio >= 0.9000
//   ||  (the tax is owner-modifiable AND can already blacklist sellers — i.e.
//        a sell-kill switch is armed even if not yet pulled).
@id("honeypot-buy-block-presign")
@severity("deny")
@reason("Simulated SELL of this token FAILS — you can BUY it but the contract will NOT let you sell it back (sell reverts / ~100% sell tax / owner-gated sell). This is the SQUID honeypot pattern. Do not sign.")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && context.custom has honeypot
  && (
    // cannot sell at all
    (context.custom.honeypot has canSell && !context.custom.honeypot.canSell)
    // sell transaction reverts in simulation
    || (context.custom.honeypot has sellReverts && context.custom.honeypot.sellReverts)
    // effective sell tax >= 90% — proceeds are dust
    || (context.custom.honeypot has sellTaxRatio
        && context.custom.honeypot.sellTaxRatio.greaterThanOrEqual(decimal("0.9000")))
    // armed sell-kill switch: tax is owner-modifiable AND owner can blacklist
    // sellers — the contract can become a honeypot the instant you are in.
    || (context.custom.honeypot has taxModifiable && context.custom.honeypot.taxModifiable
        && context.custom.honeypot has ownerCanBlacklist && context.custom.honeypot.ownerCanBlacklist)
  )
};
