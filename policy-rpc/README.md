# policy-rpc

Reference TypeScript server for policy-specific remote facts.

## Endpoints

- `GET /health`
- `GET /v1/methods`
- `POST /v1/rpc`
- `GET /debug/recent`

`oracle.usd_value` resolves a CoinGecko token USD price and computes a
`UsdValuation` result with bigint-safe scaled decimal math.

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
          "address": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
          "amount": "1000000000000000000",
          "decimals": 18
        }
      }
    ]
  }'
```
