// dambi default v2 policy — explorer id: BRG-10 (a "bridge" message moves
// no funds but changes permissions / config).
//
// LayerZero OApp calls like setDelegate / setConfig / setPeer (or a plain
// message signature) carry zero tokens, so they read as "safe — nothing leaves",
// yet they reassign the message-verification authority (delegate / DVN) and the
// trusted peer — handing over control of every future omnichain funds path. With
// no dedicated messaging decode the call lands as `Core::Unknown`: classifying
// the selector as a permission change AND extracting the delegate/peer to test
// against an allowlist both require parsing the `calldata` hex String, which
// Cedar cannot do. This is a METHOD: `bridge.permission_change` classifies the
// selector and checks the new delegate/peer against the allowlist, emitting the
// single Bool `permissionChangeUnsafe` (true = config-change message whose
// delegate/peer is NOT allowlisted).
//
// Distinct from BRG-02 (value destination): this is a zero-value authority
// change. `custom` / field are optional; the call is `optional` and fail-opens
// until the messaging-decode + allowlist surface ships. Deny when the change is
// unsafe.
@id("bridge-permission-change-deny")
@severity("deny")
@reason("Bridge message reassigns delegate/peer authority to a non-allowlisted address")
forbid(principal, action == Core::Action::"Unknown", resource)
when {
  context has custom
  && context.custom has permissionChangeUnsafe
  && context.custom.permissionChangeUnsafe
};
