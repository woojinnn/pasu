// scopeball default v2 policy — explorer id: UNI-01 (swap output redirected to
// an address other than the signer).
//
// Drainers dress a swap up as normal but flip only `recipient` to the attacker:
// you sell your tokens and someone else receives the proceeds. `recipient` is a
// base `String` field of `Amm::SwapContext` and `principal.address` is the
// evaluated wallet's address (Wallet entity attr) — both non-optional Strings —
// so this compares directly with NO enrichment and NO policy_rpc (sv = COV).
// Addresses are lowercase hex on both sides, so exact String equality holds.
@id("swap-recipient-not-self-deny")
@severity("deny")
@reason("This swap sends the bought tokens to an address that is not your wallet")
forbid(principal, action == Amm::Action::"Swap", resource)
when { context.recipient != principal.address };
