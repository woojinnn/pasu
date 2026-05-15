# adapter-registry

A local Docker-based static-file registry that mimics S3 for serving WASM
adapter artifacts. Phase 1 substrate: the directory layout, manifest generator,
and HTTP-serve mechanics — no adapters yet, no signing, no S3 deploy.

The container behaves identically to the production S3 + CloudFront origin from
the client's view; Phase 3 swaps the URL via env var, no code change.

## Layout

```
adapter-registry/
├── Dockerfile           # nginx:alpine
├── nginx.conf           # CORS, Cache-Control, gzip, /var/log/nginx → stdout
├── README.md            # this file
├── public/              # served as the nginx web root
│   ├── manifest.json    # generated; { schema_version, generated_at, adapters[] }
│   └── adapters/
│       └── <protocol>/<version>/adapter.wasm
├── scripts/
│   └── build-manifest.js
└── tests/
    └── manifest-shape.test.ts
```

## How to run locally

```bash
docker compose --profile adapter-registry up -d --build adapter-registry
curl http://127.0.0.1:8788/manifest.json
```

The default response is the empty-by-design manifest:

```json
{ "schema_version": 1, "generated_at": "...", "adapters": [] }
```

Headers worth checking:

```bash
curl -I http://127.0.0.1:8788/manifest.json
# Cache-Control: public, max-age=60
# ETag: "..."
# Access-Control-Allow-Origin: *

curl -I http://127.0.0.1:8788/adapters/<protocol>/<version>/adapter.wasm
# Cache-Control: public, max-age=31536000, immutable
# Content-Type: application/wasm
```

## How to add an adapter (Phase 1 manual flow)

1. Drop the wasm at `public/adapters/<protocol>/<version>/adapter.wasm`.
2. Author `public/adapters/<protocol>/<version>/metadata.json`:

   ```json
   {
     "display_name": "Uniswap V3",
     "supported_chains": [1, 10, 137, 8453, 42161],
     "supported_addresses": [
       { "chain_id": 1, "address": "0xE592427A0AEce92De3Edee1F18E0157C05861564" }
     ],
     "host_capabilities": ["abi_resolver.v1"]
   }
   ```

3. (Optional) Pin a stable channel in `public/adapters/<protocol>/channels.json`:

   ```json
   { "stable": "0.1.0", "canary": null }
   ```

   If absent, `stable_version` defaults to the highest semver under the
   protocol directory.

4. (Optional) Mark a version revoked by `touch`ing
   `public/adapters/<protocol>/<version>/.revoked`. The next manifest build
   flips `revoked: true` for that version. Clients refuse to load it.

5. Regenerate the manifest:

   ```bash
   node adapter-registry/scripts/build-manifest.js
   ```

   The script is idempotent — re-running over an unchanged tree produces the
   same output (modulo the `generated_at` timestamp; pin it via
   `MANIFEST_GENERATED_AT=2026-05-15T00:00:00.000Z` for reproducible CI diffs).

6. Rebuild the docker image (or hot-reload by mounting `public/` as a volume —
   the Dockerfile bakes `public/` in; for iteration speed, add
   `volumes: ["./adapter-registry/public:/usr/share/nginx/html:ro"]` under the
   `adapter-registry` compose service).

## Tests

```bash
cd adapter-registry
../extension/node_modules/.bin/vitest run
```

The suite spawns child Node processes that invoke `scripts/build-manifest.js`
against temp fixtures in `os.tmpdir()`; nothing in-repo is mutated.

See `tests/README.md` for the temporary vendored-parser arrangement and the
reconciliation plan with `extension/src/lib/adapter-manifest.ts`.

## Production note

This Docker setup behaves like S3 from the client's view. Phase 3 deploy:

- `public/` rsyncs to `s3://scopeball-adapter-registry/`
- CloudFront origin → bucket, with a `manifest.json` short-TTL behavior and a
  `*.wasm` long-TTL behavior matching the nginx rules in this directory.
- Extension reads the registry base URL from a `REGISTRY_BASE_URL` build-time
  env var; switching between local docker and production S3 is a one-flag flip.

What is intentionally **not** in Phase 1:

- adapter wasm artifact signing / verification key distribution
- the S3 + CloudFront deploy itself
- a first real adapter checked into `public/adapters/`
- any code in `extension/` or the policy-engine crates wiring into the registry
- canary channel client-side rollout logic

Each of those lands in subsequent phases.
