// scopeball default v2 policy — MetaMorpho / Morpho vault not on recognized
// curator allowlist.
// Source: policy-factory cycle 2026-06-03-morpho (protocol scope = Morpho only).
// Demand:
//   - Nov-2025 Stream Finance xUSD + Elixir deUSD depeg flowed into curated
//     MetaMorpho vaults; Morpho delisted the sdeUSD/USDC market realizing "3.6%
//     bad debt" (coinfomania, 2025-11-13); MEV Capital USDC vault took ~3.5-12%
//     depositor haircuts (blockeden / protos, 2025-11).
//   - Morpho docs: a curator force-removing a market means "any assets allocated
//     after the removal submission will be silently lost" and "the vault's share
//     price can decrease notably and instantly"
//     (docs.morpho.org/curate/concepts/security-considerations).
//   - B.Protocol: "anyone can list any market on Blue. This exposes the vault's
//     LPs to the risk of losing their funds through a malicious asset listing."
//
// EXTERNAL. A MetaMorpho vault is an ERC4626 wrapper whose curator chooses which
// Morpho Blue markets receive depositor funds (bounded only by a >=24h timelock
// and an optional guardian). The depositor cannot see this from calldata. An
// external resolver checks the vault address against a recognized-curator
// allowlist (and optionally whether any enabled market uses a depegged/synthetic
// collateral), projecting one Bool into context.custom.vaultNotRecognized.
// Unbuilt feed -> optional/fail-open. WARN.
//
// ACTION-MODEL GAP: today only LendingVenue::MorphoOptimizer (the legacy Morpho
// Optimizer vault, { vault }) carries a vault address; the NEW MetaMorpho
// ERC4626 curated vaults are not yet distinctly modeled. This draft targets
// venue == morpho_optimizer as the closest existing shape and FLAGS that a
// proper MetaMorpho venue / ERC4626 vault mapping is required (see
// proposed_method.json + registry supplement).
@id("morpho-vault-unrecognized-deposit-warn")
@severity("warn")
@reason("This Morpho vault is not on the recognized curator list — a curator can route your deposit into risky markets and the share price can drop instantly")
forbid(principal, action == Lending::Action::"Supply", resource)
when {
  context has custom
  && context.custom has vaultNotRecognized
  && context.custom.vaultNotRecognized
};
