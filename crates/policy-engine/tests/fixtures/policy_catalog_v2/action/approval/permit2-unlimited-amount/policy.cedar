// blocks: signing a Permit2 allowance for the unlimited (uint160::MAX) amount.
// why:    unlimited standing pull — Permit2 amounts are uint160, so 40 f's is the unlimited sentinel; a signed unlimited Permit2 allowance lets the spender move the wallet's full token balance for the life of the grant (Uniswap Permit2 AllowanceTransfer).
// bucket: action/approval
// tags: bucket:action cat:approval static action-family:approval venue:permit2
//
// Permit2 `amount` is uint160 (NOT uint256), so the unlimited sentinel is
// exactly "0x" + 40 'f'. Pure-static (amount rendered lower-hex `{:#x}`).
@id("permit2-unlimited-amount")
@severity("warn")
@reason("This Permit2 allowance is unlimited (uint160::MAX) — sign a bounded amount instead")
forbid(principal, action == Token::Action::"Permit2SignAllowance", resource)
when {
  context.amount == "0xffffffffffffffffffffffffffffffffffffffff"
};
