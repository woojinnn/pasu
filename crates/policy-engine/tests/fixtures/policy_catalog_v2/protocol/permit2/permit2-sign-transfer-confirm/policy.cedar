// Catalog B5 — confirm before signing a Permit2 SignatureTransfer.
// The final spender/recipient are supplied by the caller at execution time
// (not bound in the signature), so a signed transfer authorization is risky to
// hand out. Pure-static confirm.
// blocks: silently signing a Permit2 SignatureTransfer whose recipient the executor picks at execution time
// why:    Permit2 SignatureTransfer abuse — the spender/recipient are unbound in the signature, so a handed-out authorization can be redirected to an attacker (Uniswap Permit2 SignatureTransfer)
// bucket: protocol/permit2  (precedence-won)
// tags: bucket:protocol cat:permit2 static
@id("permit2-sign-transfer-confirm")
@severity("warn")
@reason("Signing a Permit2 transfer authorization — the executor chooses the recipient; confirm you trust them")
forbid(principal, action == Token::Action::"Permit2SignTransfer", resource);
