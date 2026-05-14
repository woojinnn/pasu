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
```
