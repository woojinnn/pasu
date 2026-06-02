// scopeball default v2 policy — explorer id: AMMLP-5 (cancelling an order that no
// longer exists — already filled / expired / cancelled).
//
// A cancel costs a signature (sometimes gas); a screen urging you to cancel an
// already-settled order wastes that or, worse, phishes a "looks-like-cancel"
// signature. This is a STATE2 membership check: the method
// `intent.cancel_target_missing` tests whether `orderHash` is absent from the
// wallet's active (Active/PartiallyFilled) order set and emits the Bool
// `cancelTargetMissing`. No existing policy guards cancel — this is a new axis.
// `custom` and the call are optional, so the path is `has`-guarded.
@id("ammlp-cancel-target-missing-warn")
@severity("warn")
@reason("There is no open order with this hash — cancelling it does nothing and may be a phishing prompt")
forbid(principal, action == Amm::Action::"CancelIntentOrder", resource)
when {
  context has custom
  && context.custom has cancelTargetMissing
  && context.custom.cancelTargetMissing
};
