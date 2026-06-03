// blocks: signing an unreviewably-large multi-call batch (more than 20 bundled calls)
// why:    batch-overload obfuscation — a 20+-call bundle exceeds what a user can meaningfully review pre-sign, letting a malicious leg hide among benign ones (calldata-obfuscation / consent-overwhelm threat class)
// bucket: action/batch  (precedence-won)
// tags: bucket:action cat:batch static
//
// Catalog H2 — block an unreviewably-large batch (> 20 calls).
// A 20+-call bundle cannot be meaningfully reviewed pre-sign. OUTER scope.
// Pure-static.
@id("huge-batch-deny")
@severity("deny")
@reason("This batch bundles more than 20 calls — too many to review safely; split it up")
forbid(principal, action == Core::Action::"Multicall", resource)
when { context.childCount > 20 };
