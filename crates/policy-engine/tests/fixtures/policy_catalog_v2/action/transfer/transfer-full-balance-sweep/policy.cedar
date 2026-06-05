// blocks: an ERC-20 transfer whose amount equals the wallet's entire balance of that token — a drainer-style full-balance sweep.
// why:    drainer sweep — wallet-drainer phishing typically moves the exact full balance of a token in one shot; matching the transfer amount to the on-chain balance flags the sweep before signing. Source: plan §2A transfer / blockaid drainer reports.
// bucket: action/transfer
// tags: bucket:action cat:transfer enrichment method:portfolio.balance action-family:transfer
// methods: portfolio.balance -> ../../../_methods/portfolio.balance.md
// Catalog (action/transfer) — warn when the transfer amount equals the actor's
// full balance of the token. `portfolio.balance` enriches the owner's balance as
// a raw U256 hex String; `amount` is the same canonical hex String, so an exact
// String equality is valid (no numeric compare). Guard `context.custom.balance`.
@id("transfer-full-balance-sweep")
@severity("warn")
@reason("This transfer moves your entire balance of this token — confirm it is intentional (possible wallet drain)")
forbid(principal, action == Token::Action::"Erc20Transfer", resource)
when {
  context has custom
  && context.custom has balance
  && context.amount == context.custom.balance
};
