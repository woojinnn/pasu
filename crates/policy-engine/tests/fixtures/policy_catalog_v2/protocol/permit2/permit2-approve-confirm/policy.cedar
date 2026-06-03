// Catalog A5 — confirm an on-chain Permit2 allowance grant.
//
// `Permit2.approve(token, spender, amount, expiration)` delegates an allowance
// through the Permit2 singleton rather than the token contract. Confirm so the
// user reviews the decoded spender / amount / expiry. Pure-static.
// Threat refs: Uniswap Permit2 docs; Revoke.cash Permit2 management.
// blocks: silently granting a Permit2 on-chain allowance to an unreviewed spender/amount/expiry
// why:    Permit2 allowance phishing — allowances delegated via the Permit2 singleton bypass the token contract and are a major drainer surface (Uniswap Permit2)
// bucket: protocol/permit2  (precedence-won)
// tags: bucket:protocol cat:permit2 static
@id("permit2-approve-confirm")
@severity("warn")
@reason("Granting a Permit2 allowance — confirm the spender, amount, and expiry before signing")
forbid(principal, action == Token::Action::"Permit2Approve", resource);
