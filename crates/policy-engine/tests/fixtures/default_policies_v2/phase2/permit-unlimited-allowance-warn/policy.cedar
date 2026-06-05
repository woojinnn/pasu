// Minimal buildable-TODAY tier of sig-permit-scope-reveal: STATELESS unlimited-allowance WARN.
// permitIsUnlimited from permit.amount_unlimited (readKind=derived; checks amount == uint160.max
// (Permit2) OR uint256.max (ERC-2612)). No external feed, no wallet-state, no balanceOf — ships today.
@id("permit-unlimited-allowance-warn")
@severity("warn")
@reason("This signature grants an UNLIMITED gasless allowance of this token — the entire balance, now and future, with no cap. Confirm you intend infinite spend before signing.")
forbid(principal, action == Token::Action::"Permit2SignAllowance", resource)
when {
  context has custom
  && context.custom has permitIsUnlimited
  && context.custom.permitIsUnlimited
};
