// blocks: opening a perp position whose effective leverage exceeds ~10x, where a
//         small adverse move liquidates the position.
// why:    leverage-cap config (Fireblocks transaction-authorization limits) — a
//         standing wallet policy that confines leveraged exposure. VOCAB §4/§7:
//         OpenPosition `leverage` is a Decimal STRING (no numeric/`decimal()`
//         compare possible — Cedar `decimal()` needs a string literal). The
//         on-chain leverage ceiling is instead the protocol-required initial
//         margin: leverage = 10000 / initialMarginBp. `initialMarginBp` is a Long
//         live-input on OpenPosition, so a 10x cap is enforced as a margin floor:
//         forbid when the required initial margin is below 1000 bp (10%). source:
//         Fireblocks per-asset leverage limits.
// bucket: wallet/perp-config
// tags: bucket:wallet cat:perp-config static action-family:perp-risk action-family:leverage-cap
//
// Pure-static — reads the Long `initialMarginBp` (no enrichment). A position whose
// required initial margin is < 10% implies > 10x leverage and is warned.
@id("perp-leverage-cap-config")
@severity("warn")
@reason("This perp position uses more than ~10x leverage — confirm the size and liquidation risk")
forbid(principal, action == Perp::Action::"OpenPosition", resource)
when {
  context.initialMarginBp < 1000
};
