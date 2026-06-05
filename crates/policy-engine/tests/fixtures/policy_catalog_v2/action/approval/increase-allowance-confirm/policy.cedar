// blocks: a non-standard increaseAllowance() that raises an ERC-20 allowance without an explicit confirm.
// why:    increaseAllowance is a non-EIP-20 extension that RAISES an existing allowance rather than overwriting it, so its effect (and the resulting total) is easy to under-read; surface every non-zero raise for review (EIP-20 only defines approve()).
// bucket: action/approval
// tags: bucket:action cat:approval static action-family:approval
//
// VOCAB has no separate increase_allowance UID — increaseAllowance decodes to
// Erc20Approve; this policy is bound via the manifest's erc20_approve selector.
// Warn on any raise; let a pure zero-amount no-op pass. Pure-static (amount is
// raw U256 lower-hex `{:#x}`, so zero is exactly "0x0").
@id("increase-allowance-confirm")
@severity("warn")
@reason("This raises an existing token allowance — confirm the new total spend you are granting")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when { true }
unless { context.amount == "0x0" };
