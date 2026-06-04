// BLOCKED-BY-ACTION: erc3009_transfer_with_auth surface not yet in scopeball schema (see x402 gap-spec)
// scopeball policy — a single EIP-3009 TransferWithAuthorization (the x402 exact
// payment scheme) whose `value` drains almost the entire USDC balance of the
// connected wallet in one off-chain signature.
//
// EIP-3009 is the typed-data signature x402 resource-servers ask the payer to
// sign; the facilitator then submits transferWithAuthorization(...) calldata.
// Class-identical to the Permit2 "entire-balance in one signature" drain that
// most users overlook (decrypt: "Permit2 ... authorizes access to the entire
// token balance unless the user manually sets a limit"; gate.com: "drain entire
// wallets with a single signature"; PEPE $1.39M / spWETH ~$32M single-signature
// losses). Unlike an absolute micro-payment cap, this keys on the BALANCE-
// RELATIVE share so it fits both whale and small wallets without false positives.
//
// `value` is a raw U256 hex String (NOT Cedar-comparable) and the share needs the
// wallet's CURRENT USDC balance as the denominator, so it is computed off-Cedar by
// reusing portfolio.input_fraction_bps with x402 selectors rebound: asset <- the
// EIP-712 domain.verifyingContract (USDC) carried as $.action.token, amount <-
// $.action.value. The bps Long is projected to context.custom.usdcOutflowBps.
//
// custom is optional, so the field uses the 3-part has-guard. usdcOutflowBps is a
// Long (basis points) -> compare with `>` (NOT .greaterThan/decimal). Threshold is
// user configuration; this slice inlines 5000 bps (warn when one authorization
// reaches >=50% of the held USDC). The state read lives in the sim-server fact
// host, so the call is optional and the policy fail-opens until it ships.
@id("x402-near-full-usdc-balance-warn")
@severity("warn")
@reason("This single payment authorization moves more than half of your USDC balance — one signature can empty most of your wallet")
forbid(principal, action == Token::Action::"Erc3009TransferWithAuth", resource)
when {
  context has custom
  && context.custom has usdcOutflowBps
  && context.custom.usdcOutflowBps > 5000
};
