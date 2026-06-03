// blocks: a multi-call batch that moves funds out (an erc20/nft transfer leg)
//         without the per-child recipient-allowlist check having run.
// why:    recipient-allowlist enforcement over batches (Coinbase wallet controls) —
//         a drainer hides a transfer leg inside an otherwise-benign bundle. VOCAB
//         §2/§4.4: the OUTER Multicall context exposes only a `{domain, action}`
//         summary per child — NOT each leg's `recipient` — so the actual
//         allowlist screen runs in the service-worker per-child re-eval. At outer
//         scope this is a deny gate: any batch carrying a fund-moving leg is held
//         until that per-child recipient check passes.
// bucket: wallet/batch
// tags: bucket:wallet cat:batch static action-family:recipient-allowlist action-family:batch-review
//
// OUTER scope. Pure-static — `children.containsAny` over the (domain, action)
// summary set is the only recipient-bearing signal visible here; per-leg recipient
// allowlisting is the SW per-child responsibility (VOCAB §4.4).
@id("batch-recipient-allowlist")
@severity("deny")
@reason("This batch contains a fund-transfer leg — each recipient must pass your allowlist first")
forbid(principal, action == Core::Action::"Multicall", resource)
when {
  context.children.containsAny([
    { domain: "token", action: "erc20_transfer" },
    { domain: "token", action: "nft_transfer" }
  ])
};
