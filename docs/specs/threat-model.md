# Policy Engine Threat Model

## v1 Signature Evaluation Scope

v1 evaluates off-chain EIP-712 signature requests for Permit2, EIP-2612, and
unmatched EIP-712 typed data. It is designed to catch phishing-relevant shape
and policy violations before a wallet signs:

- unknown or unapproved spenders
- wrong-chain typed-data domains
- unlimited approval amounts
- per-signature raw token or USD caps
- excessive signature deadline windows
- structurally invalid sentinel nonces

The pipeline evaluates the typed data presented to the wallet. It does not
observe chain state except through the existing `Oracle` host capability used
for USD valuation.

## Explicit v1 Non-Coverage

v1 does not detect already-used nonces.

v1 does not provide replay protection.

v1 does not detect latent EIP-2612 future-nonce permits where a signature is
valid only after earlier nonce-consuming transactions execute.

v1 nonce semantics are structural sanity only: a nonce is considered sane when
it is not equal to `type(uint256).max`. The engine does not query Permit2 nonce
bitmaps, ERC-20 `nonces(owner)`, or any equivalent on-chain nonce source.

## Deferred to v2

- On-chain nonce and bitmap lookup through a dedicated host capability
- Replay and latent permit detection
- 24h aggregate approved-USD windows
- SIWE login signature evaluation
- Adapter descriptor priority or specificity rules

## Host Capabilities

v1 adds only `Clock` to host capabilities. Existing `Approvals`, `Portfolio`,
and `StatWindows` capabilities are not part of signature evaluation in v1.
`Oracle` is reused for per-signature USD caps.
