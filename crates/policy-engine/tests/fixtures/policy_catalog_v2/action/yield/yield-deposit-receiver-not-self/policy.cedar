// blocks: a yield-token (SY) mint whose minted shares are credited to a recipient other than the depositor, redirecting the position to a third party.
// why:    4626-style deposits route the minted shares to a `recipient` argument; a dApp that sets recipient to an address the user does not control sends the yield position elsewhere (deposit-to-third-party redirection). Source: EIP-4626 deposit(receiver) semantics.
// bucket: action/yield
// tags: bucket:action cat:yield static action-family:receiver-not-self
// Pure-static: `recipient` is a required String on MintSy; compare against the
// submitter address. (No `has`-guard needed — recipient is non-optional here.)
@id("yield-deposit-receiver-not-self")
@severity("warn")
@reason("The minted yield shares go to a different address than yours — confirm the recipient is intended")
forbid(principal, action == Yield::Action::"MintSy", resource)
when {
  context.recipient != context.meta.submitter
};
