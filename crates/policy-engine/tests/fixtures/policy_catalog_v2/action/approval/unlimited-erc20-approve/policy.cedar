// blocks: signing an ERC-20 approve(spender, U256::MAX) unlimited allowance
// why:    approval phishing — an unlimited ERC-20 allowance lets the spender move the wallet's entire current and future token balance at any time, the dominant approval-drainer pattern
// bucket: action/approval  (precedence-won)
// tags: bucket:action cat:approval static
// Catalog A1 — block an unlimited ERC-20 approval.
//
// `approve(spender, U256::MAX)` is the classic drainer grant: the spender can
// move the wallet's entire (current and future) balance of the token at any
// time. `context.amount` is the raw U256 rendered lower-hex (`{:#x}`), so the
// unlimited sentinel is exactly `0x` + 64 `f`. Pure-static — no enrichment.
// Threat refs: Chainalysis approval-phishing 2023; Revoke.cash; kalis.me.
@id("unlimited-erc20-approve")
@severity("deny")
@reason("Unlimited ERC-20 approval (U256::MAX) lets the spender drain your full token balance — approve only the amount you need")
forbid(principal, action == Token::Action::"Erc20Approve", resource)
when {
  context.amount == "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
};
