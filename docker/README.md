# Docker dev environment

Single long-running container with the workspace mounted. Carries Rust
1.83 + wasm32 target + wasm-pack + wasm-bindgen-cli, plus Node 20 with
corepack-enabled Yarn 4 for the extension. Headless Chromium for
`wasm-pack test --headless --chrome` and Playwright.

## Quickstart

```sh
./scripts/dev-up.sh           # build + attach a shell
./scripts/test-all.sh         # rust + clippy + fmt + extension (when present)
./scripts/wasm-build.sh       # build WASM and copy into extension/
```

## Layout

| File | Purpose |
|------|---------|
| `docker/Dockerfile.dev` | Image definition (Rust + Node + wasm-pack + Chromium) |
| `docker-compose.yml` | One service `dev` with persistent volumes for cargo + yarn |
| `scripts/dev-up.sh` | Build + start + drop into shell |
| `scripts/test-all.sh` | Full test/lint/build sweep |
| `scripts/lint.sh` | Auto-fix formatters and clippy |
| `scripts/wasm-build.sh` | wasm-pack build + copy into extension/ |

## Persistent caches (named volumes)

Compose mounts these so cargo/yarn caches survive `docker compose down`:

- `cargo-registry`, `cargo-git`, `cargo-target` — cargo build cache
- `yarn-cache` — yarn 4 cache
- `extension-node-modules` — extension's node_modules

Wipe everything with `docker compose down -v`.

## CI

`.github/workflows/ci.yml` runs the same suite on GitHub-hosted Ubuntu
runners. The `wasm` and `extension` jobs probe for crate / package.json
existence so they auto-skip until those land in later plans.
