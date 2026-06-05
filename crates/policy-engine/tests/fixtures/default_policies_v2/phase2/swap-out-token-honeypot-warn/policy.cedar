// scopeball default v2 policy — explorer id: UNI-05 (buying an unverified /
// honeypot token).
//
// An impostor token (name-spoof) or a honeypot (buyable but not sellable) turns
// the proceeds to dust. If the token is already held, wallet state classifies it
// (`TokenHolding.kind == Unknown`); for an UNHELD `tokenOut`, classification and
// honeypot detection need an external registry/sim. This is sv = PART: the
// method `token.swap_out_classification` folds the held-state read with the
// external probe and emits BOTH flags (record), projected into `context.custom`.
// The call is `optional` (fail-open until the registry ships) and `custom` is
// optional, so each flag is `has`-guarded before reading.
@id("swap-out-token-honeypot-warn")
@severity("warn")
@reason("The token you are about to receive is unverified or may be a honeypot")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && (
    (context.custom has outTokenUnclassified && context.custom.outTokenUnclassified)
    || (context.custom has outTokenHoneypot && context.custom.outTokenHoneypot)
  )
};
