// Catalog B4 — confirm before signing a Permit2 allowance (off-chain).
// A signed Permit2 allowance grants `spender` an off-chain-authorized allowance
// on the token — a major phishing surface. Pure-static (spender/amount decoded).
// blocks: silently signing an off-chain Permit2 allowance that grants a spender token allowance
// why:    Permit2 off-chain allowance phishing — a signed allowance authorizes a spender without an on-chain tx, a high-volume wallet-drainer vector (Uniswap Permit2)
// bucket: protocol/permit2  (precedence-won)
// tags: bucket:protocol cat:permit2 static
@id("permit2-sign-allowance-confirm")
@severity("warn")
@reason("Signing a Permit2 allowance — confirm the spender and amount before approving")
forbid(principal, action == Token::Action::"Permit2SignAllowance", resource);
