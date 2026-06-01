# simulation-sync layout

`simulation-sync` owns external data refresh. It is native-only and should not
be pulled into WASM policy evaluation.

## Directories

- `src/actions/`: walks an `Action` and resolves action-specific live inputs.
- `src/live/`: generic `LiveField<DataSource>` pipeline: walk, batch, fetch,
  derive, and write values back.
- `src/sources/`: external adapters plus authoritative primitive snapshot sync.
  RPC, oracle, registry, venue, discovery, block subscription, and bulk
  primitive refresh live here.
- `src/manifests/`: declarative `live_inputs` manifest parsing and placeholder
  resolution.
- `src/runtime/`: config, error type, orchestrator, and polling scheduler.

Root-level modules such as `simulation_sync::fetchers` and
`simulation_sync::orchestrator` are compatibility re-exports. New code should
prefer the directory-aligned paths.

## External fetch model

Field-level external values use `LiveField<T>` with a `DataSource`. Wallet
primitive sync is different today: balances, approvals, block heights, and
Hyperliquid account snapshots are authoritative state snapshots and are updated
in bulk from `src/sources/primitives.rs` or venue fetchers. The common long-term
direction is to lower primitive sync into planned source requests so both paths
can share batching and dispatch without forcing every primitive state field to
become a `LiveField`.
