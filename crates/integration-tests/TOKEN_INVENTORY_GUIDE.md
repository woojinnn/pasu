# Token Inventory Guide

> Protocol onboarding token-surface guide. This file is tracked and self-contained because `registryV2/docs/*.md` may be local research notes in some worktrees. Use it whenever a protocol creates, wraps, issues, or directly uses user-held tokens.

## Why This Is Part Of Protocol Onboarding

`registryV2/tokens/<chainId>/<addr>.json` is not passive metadata. It is the build-time input for ERC standard manifests:

```jsonc
"match": {
  "selector": "0xa9059cbb",
  "chain_to_addresses_source": "tokens:erc20",
  "chain_ids": [1, 8453]
}
```

`registryV2/scripts/build-index.ts` loads `tokens/<chainId>/*.json`, groups addresses by `erc_kind`, and expands `tokens:erc20` / `tokens:erc721` / `tokens:erc1155` into concrete `(chain, address, selector)` callkeys. If a protocol-issued token is missing here, standard ERC calls to that address can route to `no_declarative_v3_mapper`.

Therefore protocol onboarding must include token inventory when the protocol has user-held LP/share/receipt/debt/governance/base tokens.

## Scope Decision

Register every in-scope token contract that the protocol creates or directly relies on:

| Protocol shape | Token inventory target |
|---|---|
| AMM fungible LP: Curve, Balancer, Aerodrome, Uniswap V2 | LP token / pool share token, governance token, pool underlyings |
| Concentrated AMM: Uniswap V3/V4, Trader Joe LB | position-manager collection only; never individual token IDs |
| Lending: Compound, Aave, Spark, Morpho-style wrappers | yield receipt tokens, debt receipt tokens, reserve underlyings, governance token |
| LST/LRT | staking/wrapped tokens, governance token, underlying token/native ref |
| Airdrop/launchpad | actual issued points/token contracts; claim rights are not token contracts |

Large pool protocols may be batched, but the P0 log must state the boundary:

```jsonc
{
  "token_surface": {
    "included_pools": ["<pool addr>", "..."],
    "deferred_pools_source": "<official pool list URL>",
    "deferred_reason": "long-tail batch; no silent omission"
  }
}
```

## Required Sources

Use static first-party or verified sources:

- official deployment/address-book page
- official token list or pool list JSON
- official GitHub deployment artifacts
- verified explorer token page for symbol/name/decimals

Do not use ad-hoc RPC reads for symbol/decimals as the primary source. Do not infer underlyings, rebasing behavior, pool pairs, or receipt semantics from memory.

## File Shape

Path:

```text
registryV2/tokens/<chainId>/<lowercase-address>.json
```

Minimum ERC20/native shape:

```jsonc
{
  "erc_kind": "erc20",
  "chainId": 1,
  "address": "0x<40 lowercase hex>",
  "symbol": "USDC",
  "decimals": 6,
  "name": "USD Coin",
  "source": "https://<first-party-or-verified-source>",
  "token_kind": {
    "kind": "base",
    "category": { "kind": "stable" },
    "peg_to": { "kind": "fiat", "value": "usd" }
  }
}
```

`erc_kind` controls auto-enumeration and must be one of:

- `erc20`
- `erc721`
- `erc1155`
- `native`

`token_kind` is semantic metadata for policy/user interpretation. Common values:

- `base`
- `native_gas`
- `wrapped`
- `lp_share`
- `yield_receipt`
- `debt_receipt`
- `stake_receipt`
- `points_token`
- `maturity_note`
- `unknown`

## Common TokenKind Mapping

Curve / Balancer / Aerodrome / Uniswap V2 fungible LP:

```jsonc
{
  "kind": "lp_share",
  "pool": {
    "protocol": { "name": "curve_stableswap_ng", "chain": "eip155:1" },
    "pool_addr": "0x<pool-or-lp-token>"
  },
  "underlyings": [
    { "key": { "standard": "erc20", "chain": "eip155:1", "address": "0x<coin0>" } },
    { "key": { "standard": "erc20", "chain": "eip155:1", "address": "0x<coin1>" } }
  ],
  "share_form": "fungible",
  "shape": { "kind": "pooled" }
}
```

Compound-style cToken / ERC4626 indexed receipt:

```jsonc
{
  "kind": "yield_receipt",
  "protocol": { "name": "compound_v3", "chain": "eip155:1" },
  "underlying": {
    "key": { "standard": "erc20", "chain": "eip155:1", "address": "0x<underlying>" }
  },
  "rebase_form": "index"
}
```

Aave-style aToken rebasing receipt:

```jsonc
{
  "kind": "yield_receipt",
  "protocol": { "name": "aave_v3", "chain": "eip155:1" },
  "underlying": {
    "key": { "standard": "erc20", "chain": "eip155:1", "address": "0x<underlying>" }
  },
  "rebase_form": "rebasing"
}
```

Debt receipt:

```jsonc
{
  "kind": "debt_receipt",
  "protocol": { "name": "aave_v3", "chain": "eip155:1" },
  "underlying": {
    "key": { "standard": "erc20", "chain": "eip155:1", "address": "0x<underlying>" }
  },
  "rate_mode": "variable"
}
```

Position-manager collection:

```jsonc
{
  "erc_kind": "erc721",
  "chainId": 1,
  "address": "0x<position-manager>",
  "collection_name": "Uniswap V3 Positions NFT",
  "symbol": "UNI-V3-POS",
  "source": "https://<verified-source>",
  "token_kind": {
    "kind": "lp_share",
    "pool": { "protocol": { "name": "uniswap_v3", "chain": "eip155:1" } },
    "underlyings": [],
    "share_form": "non_fungible",
    "shape": { "kind": "concentrated" }
  }
}
```

## Referential Rule

Any `underlying` or `peg_to.token` reference must exist in `registryV2/tokens` too. If a Curve LP token references crvUSD and FRAX, both underlying token JSON files must be present or added in the same token-surface batch.

Current build-index enforcement is intentionally shallow: it validates JSON object shape, `erc_kind`, `address`, positive `chainId`, native sentinel, and directory/field chain match. It does not yet enforce filename/address match, `source`, `symbol`, `decimals`, `token_kind` variants, or underlying referential integrity. Treat those as reviewer-enforced until a dedicated `check:tokens` gate exists.

## Curve-Specific Checklist

For Curve onboarding, research and register:

- CRV and crvUSD when in chain scope
- every covered pool's LP token/pool token
- every covered pool's underlying coins
- gauge/stake/Convex-style receipt tokens when the onboarding scope includes user-held staking/gauge flows
- long-tail pool exclusions or deferred batches in the P0 log

Curve pool contracts are often ERC20 LP tokens themselves. Treat them like Compound cTokens at the inventory level: user-held protocol-issued share/receipt contracts must be in the token registry so ERC standard calls can route.

## Validation

After token edits:

```bash
cd registryV2
npm run build
```

This validates token JSON shape enough for build-index and proves ERC source expansion emits concrete callkeys. Then run the normal onboarding gates:

```bash
cd registryV2 && npm run check:manifest && npm run check:surface
cd browser-extension && node .yarn/releases/yarn-4.14.1.cjs vitest run --root ../registryV2 scripts/__tests__/build-index.test.ts
cargo test -p policy-engine-integration-tests --test v3_decode_harness
```

P2 real-tx feedback loop:

- if a token contract call returns `no_declarative_v3_mapper`, check whether the token address is absent from `registryV2/tokens/<chainId>/`
- if present, check `erc_kind` matches the standard selector (`erc20` vs `erc721`/`erc1155`)
- if it is a protocol-specific token whose standard selector has protocol-specific semantics, ensure concrete protocol manifests override standard sourced manifests
