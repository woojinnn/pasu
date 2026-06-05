// blocks: a collection-wide NFT operator grant (setApprovalForAll) hidden inside a multi-call batch
// why:    NFT-drainer vector — setApprovalForAll hands an operator control over an entire ERC-721/1155 collection; bundling it in a batch hides the grant from pre-sign review (operator-approval / batch-obfuscation threat class)
// bucket: action/batch  (precedence-won)
// tags: bucket:action cat:batch static
//
// Catalog H5 — warn when a batch hides a collection-wide NFT operator grant.
// `setApprovalForAll` bundled inside a multicall is a stealthy NFT-drainer vector.
// OUTER scope. Pure-static.
@id("batch-bundled-setapprovalforall")
@severity("warn")
@reason("This batch contains a collection-wide NFT approval (setApprovalForAll) — review the operator")
forbid(principal, action == Core::Action::"Multicall", resource)
when {
  context.children.contains({ domain: "token", action: "nft_set_approval_for_all" })
};
