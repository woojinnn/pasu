// dambi default v2 policy — explorer id: UNI-13 (fee-on-transfer / rebasing
// token swapped via an unsupported router).
//
// FoT (transfer-tax) and rebasing tokens change amount on every transfer, so an
// unsupporting V3/V4 router reverts or hides loss as inflated slippage. Rebasing
// is partly known from wallet state (`TokenHolding.kind == Wrapped{PegKind::Rebasing}`)
// but FoT needs an external token-tax registry — sv = PART. The method
// `token.swap_special_token` folds the held-state peg read with the external FoT
// probe and emits BOTH flags (record), projected into `context.custom`. `custom`
// and the call are optional, so each flag is `has`-guarded.
@id("swap-special-token-fot-rebasing-warn")
@severity("warn")
@reason("The token you are swapping is fee-on-transfer or rebasing — many routers mishandle it")
forbid(principal, action == Amm::Action::"Swap", resource)
when {
  context has custom
  && (
    (context.custom has specialToken && context.custom.specialToken)
    || (context.custom has feeOnTransfer && context.custom.feeOnTransfer)
  )
};
