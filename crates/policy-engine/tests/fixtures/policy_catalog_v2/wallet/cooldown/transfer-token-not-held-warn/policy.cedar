// blocks: an ERC-20 transfer of a token the wallet holds a zero balance of (the actor does not own this token).
// why:    Fireblocks-style "don't move what you don't hold" guardrail — a transfer of a not-held token is either a phantom / dust-airdrop scam token whose approval the user is being tricked into exercising, or a wrong-account / wrong-token mistake; either way it is never a legitimate spend (zero-balance-token threat class).
// bucket: wallet/cooldown
// tags: bucket:wallet cat:cooldown enrichment method:portfolio.balance action-family:cooldown
// methods: portfolio.balance -> ../../../_methods/portfolio.balance.md
//
// Cooldown — warn when the actor's on-chain balance of the transferred token is
// exactly zero. `portfolio.balance` enriches the balance as a raw U256 hex String;
// an exact String compare against the zero sentinel "0x0" is valid (no numeric
// compare). Registered method — active (not dormant). Guard custom.balance.
@id("transfer-token-not-held-warn")
@severity("warn")
@reason("You hold none of this token — confirm you are sending the right asset from the right account")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has balance
  && context.custom.balance == "0x0"
};
