// blocks: signing an off-chain intent order without the user reviewing the sell/buy amounts, minimum out, and recipient first
// why:    blind off-chain-order signing — a signed intent (UniswapX / CowSwap / 1inch Fusion) is a solver-fillable mandate; an unreviewed order can carry an adverse min-out or attacker recipient
// bucket: protocol/intent-venue  (precedence-won)
// tags: bucket:protocol cat:intent-venue static
// Catalog B7 — confirm before signing an intent order (UniswapX / CowSwap / 1inch Fusion).
// A signed off-chain order is filled later by a solver; confirm the sell/buy and
// recipient before signing. Pure-static.
@id("intent-order-sign-confirm")
@severity("warn")
@reason("Signing an off-chain intent order — confirm the sell amount, minimum out, and recipient")
forbid(principal, action == Amm::Action::"SignIntentOrder", resource);
