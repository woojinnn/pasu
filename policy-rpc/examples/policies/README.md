# Historical reference policies

These files document the v0 "policy-bundled `.policy-rpc.json`" pattern (one
manifest per policy). The current model uses per-RPC-endpoint manifests and
relies on the Cedar validator against the enriched schema to catch missing
demand fields at install time.

Cedar bodies here have been updated to reference `context.custom.*` and use
`has` guards, per spec D3 and D8.
