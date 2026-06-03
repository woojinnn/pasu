// blocks: a debt position being opened without the user reviewing which asset and how much is being borrowed.
// why:    borrowing pledges collateral and creates ongoing liquidation exposure — a class of irreversible financial commitment a user should confirm at sign time.
// bucket: action/lending  (precedence-won)
// tags: bucket:action cat:lending static
// Catalog E1 — confirm before opening a debt position.
// Borrowing creates liquidation exposure; confirm the asset and amount.
// Pure-static.
@id("borrow-confirm")
@severity("warn")
@reason("Borrowing against your collateral opens liquidation risk — confirm the asset and amount")
forbid(principal, action == Lending::Action::"Borrow", resource);
