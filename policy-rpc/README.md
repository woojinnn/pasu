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
