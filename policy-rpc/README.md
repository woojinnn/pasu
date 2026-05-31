# policy-rpc (LEGACY — archived)

> **Status: archived as of Phase 8B (2026-05).** The browser extension no
> longer requires this server for the default policy set; the new Rust
> backend at `crates/simulation/server` (`POST /evaluate` on `:8788`)
> handles wallet-state simulation, and the one default policy that
> previously called `oracle.usd_value` (`large-swap-usd-warning`) is
> inert pending a LiveField-first rewrite. The directory is kept for
> historical reference (one method, `oracle.usd_value`, has a real
> CoinGecko/Chainlink implementation that the LiveField sync layer
> may want to mine for fetchers) but the server is not started or
> tested by the build pipeline.

Reference TypeScript server for policy-specific remote facts.

## Endpoints

- `GET /health`
- `GET /v1/methods`
- `POST /v1/rpc`
- `GET /debug/recent`

`oracle.usd_value` resolves a token USD price and computes a
`UsdValuation` result with bigint-safe scaled decimal math. Two
sources are wired:

- `coingecko` (default) — HTTP. Works with no extra config.
- `chainlink` — reads on-chain AggregatorV3 feeds via `eth_call`. The
  bundled feed table covers ETH/BTC/USDC/USDT/DAI on Ethereum mainnet
  plus wrapped-native on Optimism / Arbitrum / Base / Polygon (extend
  by editing `CHAINLINK_FEEDS` in `src/chainlink-client.ts`). RPC
  endpoints fall through three layers:

  1. **User-supplied via env** —
     `POLICY_RPC_CHAIN_RPCS='{"1":"https://eth-mainnet.alchemy.com/v2/<key>"}'`.
     Accepts a single URL string or `string[]` per chain; multiple
     entries are tried in order with automatic failover.
  2. **Bundled public RPC defaults** — when the env doesn't cover a
     chain, the client falls back to a curated list of 3-4 public
     endpoints per chain (llamarpc / publicnode / ankr / chain
     foundation). Production traffic should still use a private
     provider — public endpoints have aggressive rate limits and no
     SLA.
  3. **Strict mode** — set `POLICY_RPC_DISABLE_PUBLIC_RPCS=1` to skip
     the bundled defaults entirely. Chains without env-supplied URLs
     return `unsupported_chain` instead of leaking traffic to public
     endpoints. Use this in production when policy requires all RPC
     traffic stay within a specific provider.

  Per-request timeout is 10s; failed endpoints fall over to the next
  in the list. Tokens without a feed entry return `not_found`.

The reference server also exposes v1 mock methods for host-capability-shaped
facts while the backing services are still being designed:

- `clock.now`
- `approval.allowance`
- `approval.cover_inputs`
- `portfolio.balance`
- `portfolio.input_fraction_bps`
- `oracle.effective_rate_bps`
- `stat_window.snapshot`
- `stat_window.swap_stats`

`schema/policy-schema/extensions/DEX/swap.policy-rpc.json` shows how the legacy swap
enrichment fields can be requested and projected through policy-rpc manifests.

## Development

```bash
../extension/node_modules/.bin/vitest run
```

The implementation uses Node built-ins and the global `fetch`; it has no runtime
dependencies.

## Docker

Build and run the RPC server from the repository root:

```bash
docker build -f policy-rpc/Dockerfile -t scopeball-policy-rpc .
docker run --rm -p 8787:8787 scopeball-policy-rpc
```

Or use the compose service:

```bash
docker compose --profile policy-rpc up policy-rpc --build
```

The image listens on `0.0.0.0:8787` by default. Override `PORT` if the server
should bind to a different container port.

```bash
curl http://127.0.0.1:8787/health
curl http://127.0.0.1:8787/v1/methods
curl -sS http://127.0.0.1:8787/v1/rpc \
  -H 'content-type: application/json' \
  -d '{
    "request_id": "manual-test-1",
    "calls": [
      {
        "id": "call-1",
        "method": "oracle.usd_value",
        "params": {
          "chain_id": 1,
          "asset": {
            "kind": "erc20",
            "address": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
            "symbol": "WETH",
            "decimals": 18
          },
          "amount": "1000000000000000000"
        }
      }
    ]
  }'
```

`oracle.usd_value` also accepts the older flat `{ chain_id, address, amount,
decimals }` shape. The asset-object shape is what default swap policy manifests
emit.
