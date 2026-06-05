// blocks: a swap that routes its output to an address other than the sender, where the proceeds leave the user's own wallet
// why:    drainer / output-redirection — malicious dApps set the swap recipient to an attacker address so the swapped funds never reach the signer
// bucket: action/swap  (precedence-won)
// tags: bucket:action cat:swap static degenerate-allowlist
// Catalog C4 — warn when swap output is routed to an address other than the sender.
// Drainers route swap proceeds to themselves. `recipient` and `meta.submitter`
// are both lowercase 0x addresses. Pure-static.
@id("swap-recipient-not-self")
@severity("warn")
@reason("Swap output goes to an address other than your own — confirm the recipient")
forbid(principal, action == Amm::Action::"Swap", resource)
when { context.recipient != context.meta.submitter };
