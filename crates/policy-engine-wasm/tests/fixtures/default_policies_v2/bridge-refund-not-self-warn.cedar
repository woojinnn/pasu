// dambi default v2 policy — explorer id: BRG-12 (bridge refund / recovery
// address is not the user's own wallet).
//
// On a failed fill or unreceivable destination, funds return to the bridge's
// refund / recovery address. That fallback address is a SEPARATE calldata
// argument the user almost never inspects — if it is an attacker address or a
// contract that cannot receive a refund, a failed transfer becomes a permanent
// loss. With no dedicated bridge decode the call lands as `Core::Unknown`:
// `refundAddress` is buried in the `calldata` hex String, distinct from
// `recipient`, so BRG-01's `recipient`-vs-self logic does NOT cover it and Cedar
// cannot extract it. This is a METHOD: `bridge.refund_self` introspects the
// calldata for the refund/recovery address and compares it to the wallet owner,
// emitting the Bool `refundNotSelf` (true = refund address is not the wallet).
//
// Distinct from BRG-01 (the success-path recipient): this is the failure-path
// fallback, invisible on a normal trade. `custom` / field are optional; the call
// is `optional` and fail-opens until the refund-introspection surface ships.
// Deny when the refund address is not the wallet.
@id("bridge-refund-not-self-warn")
@severity("warn")
@reason("Bridge refund address is not your wallet — a failed transfer would be unrecoverable")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has refundNotSelf
  && context.custom.refundNotSelf
};
