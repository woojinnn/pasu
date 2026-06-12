# `surface/` — executable research-completeness gate

This directory is the **independent ground truth** for the surface-completeness
gate (`scripts/check-surface-completeness.ts`, `npm run check:surface`). It
converts the P0 onboarding step "enumerate every external state-changing
function + triage each COVER/EXCLUDE"
(`crates/integration-tests/PROTOCOL_ONBOARDING_AND_TESTING.md` §3) from a PROSE
checklist into a **build-enforced invariant**.

## Why it exists

A prose checklist can be skipped with no signal. During the Morpho dogfood the
agent authored adapters for the "easy" lending verbs (supply/withdraw/borrow/
repay) and silently dropped `setAuthorization` — the permission-delegation
primitive, which is Dambi's entire reason to exist. Nothing failed; the miss
was invisible until a human reviewer caught it.

This gate makes that class of omission a **build failure**, the same way
`compose_per_policy` makes a missing Cedar registration a runtime
`MissingAction`. Research-completeness becomes machine-checked, not trusted.

## Why an independent snapshot (not just coverage vs manifests)

Checking authored coverage against authored manifests is **circular**: forget a
function in BOTH and it passes. So the gate diffs against a source neither the
manifests nor the triage can lie about — the contract's **verified full ABI**,
fetched once from a 1st-party source and committed here. The snapshot has every
external function; the gate forces an explicit decision on each.

## Two completeness layers — contracts (I0) and functions (I1~I3)

The gate enforces completeness on **two axes**, because they fail differently:

- **Function coverage (I1~I3)** — *within a contract you found*, did you triage every external function? Ground truth = the verified ABI (cannot omit a function that exists).
- **Contract inventory (I0)** — did you find *every contract*? Ground truth = the official deployment list. **I1 and the adapter-blind real-tx pull are both blind here**: I1 has nothing to run on a contract you never snapshotted, and the real-tx pull queries `txlist&address=` so a contract whose address research never found is never even fetched. I0 closes that — but only as well as the official list (which *can* omit a contract; honest floor below).

## Layout

```
surface/<protocol>/_deployments.json          ← I0: 1st-party deployment list (contract inventory)
surface/<protocol>/_address_universe.json     ← U0: factory/pool/vault child address universe (optional but mandatory for pool-heavy onboarding)
surface/<protocol>/_pool_universe.json        ← U0 alias for pool-specific protocols
surface/<protocol>/<contract>.abi.json        ← snapshot (function ground truth)
surface/<protocol>/<contract>.coverage.json   ← triage (function decisions)
```

### `_deployments.json` — contract inventory (I0 ground truth)

The protocol's **official deployment list** as data. EVERY deployed contract gets
an explicit `cover` (you will snapshot + triage its functions) or `exclude` (with
a reason: implementation-behind-proxy / oracle / governance / infra / standard
token). This is the independent source that forces a decision on each contract —
the same role the verified ABI plays for functions.

```jsonc
{
  "protocol": "lido",
  "source": "docs.lido.fi/deployed-contracts",     // 1st-party page
  "url": "https://docs.lido.fi/deployed-contracts/",
  "fetchedAt": "2026-05-31",
  "contracts": [
    { "name": "stETH (Lido, proxy)", "chainId": 1, "address": "0xae7ab9…", "decision": "cover",   "reason": "user staking-token surface" },
    { "name": "Accounting Oracle",   "chainId": 1, "address": "0x852ded…", "decision": "exclude", "reason": "oracle: committee report, not user pre-sign" }
    // … every contract on the official page
  ]
}
```

**Where to get the list (1st-party first):**
- **1차**: official docs Deployments/Addresses page · official GitHub deploy artifacts (`@aave/address-book`, Uniswap deployments JSON, hardhat-deploy `deployments/`, foundry `broadcast/`) · on-chain registries (Curve `AddressProvider`, Aave `PoolAddressesProvider`).
- **2차 (discovery cross-check, verify against 1차)**: DefiLlama-Adapters GitHub (lists addresses per protocol) · Dune `<project>` decoded namespaces · Etherscan/Basescan address labels / Label Cloud · Sourcify verified repo.
- No single complete+authoritative registry exists — use the 1st-party deploy artifact as `_deployments.json`, and the aggregators as a sweep to *challenge* it.

### `<contract>.abi.json` — snapshot

Raw verified ABI + provenance. Immutable; re-fetch to verify it.

```jsonc
{
  "source": "etherscan",
  "url": "https://api.etherscan.io/v2/api?chainid=1&module=contract&action=getabi&address=0x…",
  "chainId": 1,
  "address": "0x…",          // lowercase
  "contract": "Morpho",
  "fetchedAt": "2026-05-31",
  "note": "non-proxy singleton; ground-truth external surface",
  "abi": [ /* the verified ABI, verbatim */ ]
}
```

### `<contract>.coverage.json` — triage

The §3 triage table as data. EVERY external-mutating selector in the snapshot
must appear under `functions` with an explicit `cover | exclude` + `reason`.

```jsonc
{
  "contract": "Morpho",
  "chainId": 1,
  "address": "0x…",                 // single-address mode (must equal the snapshot's); see addresses[] below for factory pools
  "snapshot": "morpho-blue.abi.json",
  "functions": {
    "0xa99aad89": { "name": "supply",     "decision": "cover",   "reason": "user position: supply assets" },
    "0x13af4035": { "name": "setOwner",   "decision": "exclude", "reason": "governance (owner-only)" }
    // … every external-mutating selector, no exceptions
  },
  "signed_structs": {                 // EIP-712 messages a user SIGNS (off-chain)
    "Authorization": { "decision": "cover", "reason": "off-chain grant; authorization-sign manifest" }
  }
}
```

### Factory pools — one ABI, many addresses (`addresses[]`)

For factory-deployed protocols where users call each instance **directly** (Curve:
one StableSwap-NG implementation ABI, N pool addresses), use `addresses[]` instead
of a single `address`. The one `snapshot` is the shared **implementation** ABI:
**I1 runs once** against it, while **I2/I3/S1/S2 run against EVERY pool** — each
pool must carry the cover-selector manifests, so a single pool missing a manifest
fails the gate. (Router/singleton entry points like Uniswap's router don't need
this — they're one address.)

```jsonc
{
  "contract": "StableSwap-NG",
  "chainId": 1,
  "addresses": ["0xpoolA…", "0xpoolB…", "0xpoolC…"],  // share the one snapshot ABI
  "snapshot": "stableswap-ng.abi.json",               // the IMPLEMENTATION ABI
  "functions": { /* … the shared surface, triaged once … */ }
}
```

### `_address_universe.json` / `_pool_universe.json` — child address universe

For protocols where users directly call many factory-created or registry-listed
children (Curve pools, Balancer pools, lending vault factories, pair factories),
contract inventory alone is not enough. The registry routes by exact
`(chainId,to,selector)`, so a missing child address is a production miss even
when the ABI and selector are already known.

Use `_address_universe.json` for generic child universes, or `_pool_universe.json`
when "pool" is the natural protocol term. The checker is protocol-agnostic:

```jsonc
{
  "protocol": "curve",
  "kind": "pool",
  "source": "Curve public API getPools/all/ethereum",
  "source_count": 2260,
  "batch_boundary": "ethereum:getPools/all snapshot 2026-06-01",
  "candidates": [
    {
      "chainId": 1,
      "address": "0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7",
      "decision": "defer",
      "reason": "legacy StableSwap-v1 batch; not in this onboarding slice",
      "batch": "curve-legacy-stableswap-v1"
    },
    {
      "chainId": 1,
      "address": "0x16c6521dff6bab339122a0fe25a9116693265353",
      "decision": "cover",
      "reason": "RouterNG user swap entrypoint"
    }
  ]
}
```

Run:

```bash
npm run check:universe
npm run check:universe -- --protocol curve
npm run check:universe -- --protocol curve --require-cover-linkage
```

`--protocol <name>` fails if that protocol has no universe artifact. Use it in
onboarding P0/P4 for pool/factory/vault-heavy protocols. `--require-cover-linkage`
is the P4 form: it additionally requires every `cover` address to have generated
`index/by-callkey` entries after `npm run build`.

## Invariants (any violation → `npm run check:surface` exits 1)

| ID  | Rule | Catches |
|-----|------|---------|
| **I0**  | every `_deployments.json` `cover` contract has a surface snapshot; every `exclude` has a reason (opt-in: no `_deployments.json` → visible WARN, contract-inventory not enforced) | a **contract** research never found (invisible to I1 + the address-keyed real-tx pull) |
| **I1**  | every snapshot external-mutating selector (`type==="function"` && `stateMutability ∈ {nonpayable,payable}`) has a `functions` entry | the original miss — a function nobody triaged |
| **I1'** | every `functions` key is a real selector that exists in the snapshot | stale / typo'd coverage |
| **I2**  | every `cover` selector has a manifest at `(chain,address,selector)` — for **each** address in `addresses[]` | triaged-as-cover but adapter never built (at any pool) |
| **I3**  | every on-chain manifest selector at `(chain,address)` is `cover` here — checked per address | a manifest for a function you marked exclude / never triaged |
| **S1/S2** | each typed-data manifest `primary_type` ↔ a `signed_structs` cover (both ways) | off-chain EIP-712 grant un-triaged |
| **U0** | every candidate in `_address_universe.json` / `_pool_universe.json` has `cover | exclude | defer` + reason; optional P4 linkage checks every cover address has generated callkeys | representative-pool onboarding disguised as full protocol support |

## Onboarding a new protocol

**Step 0 — contract inventory (I0).** Before functions, enumerate the protocol's
*contracts*. Pull the official deployment list (1st-party), challenge it with a
DefiLlama/Dune/Etherscan-labels sweep, and write `surface/<protocol>/_deployments.json`
— every contract `cover` or `exclude:reason`. `npm run check:surface` I0 then
proves no user-facing contract was dropped. (Skip and you get a WARN, not a fail —
but the contract-inventory blind spot stays open.)

**Then, for each `cover` contract (4 steps):**

1. **Fetch** the verified ABI from a 1st-party source (key is local-only, never
   committed — `crates/integration-tests/.env`):
   ```bash
   set -a; source crates/integration-tests/.env; set +a
   curl -s "https://api.etherscan.io/v2/api?chainid=<CHAIN>&module=contract&action=getabi&address=<ADDR>&apikey=${ETHERSCAN_API_KEY}"
   ```
2. **Snapshot** — wrap the raw ABI with provenance → `surface/<protocol>/<contract>.abi.json`.
3. **Triage** — for EVERY external-mutating function, write `cover` (you will
   build a manifest) or `exclude` (with a reason: governance / keeper / infra /
   relayer). Permission primitives (`authorize|approve|permit|delegate|setOperator|
   setApprovalForAll`) are **never** exclude — they are the analyzer's purpose.
   **Exception — generic ERC-20/721 standard funcs.** A contract that is itself a
   standard token (Curve pool-as-LP, Lido stETH/wstETH, unstETH NFT) exposes
   `approve`/`permit`/`transfer`/`setApprovalForAll` that the `tokens:erc20` /
   `erc721` **standard adapter already analyzes** via auto-enumerate (out of gate
   scope, see §Scope). Those are `exclude` with reason "standard ERC-20/721 —
   erc20/erc721 standard adapter" (NOT cover — a `cover` would fail I2 since the
   auto-enumerate manifest is not a per-`(chain,address)` manifest). The "never
   exclude" rule targets **protocol-specific** grants no standard adapter covers
   (`setAuthorization`, credit `delegate`, operator maps).
   Add off-chain EIP-712 messages under `signed_structs`.
4. **Gate** — `npm run check:surface`. Fix every `✗` until it PASSes. Build the
   `cover` manifests; the gate then proves nothing was dropped.

## Scope & limits (honest)

- **Incremental, never silent.** Only contracts with a snapshot here are
  enforced. Contracts with manifests but no snapshot are reported as a visible
  `UNGATED` WARN (not a failure) — onboard them protocol by protocol.
- **I0 floor — weaker than I1.** A verified ABI cannot omit a function that
  exists, so I1 is complete-by-construction. A deployment *page* CAN omit a
  contract, so I0 is only as complete as the official list. I0 moves the
  single-point-of-failure from "agent's memory" to "official list + aggregator
  cross-check" — better, not airtight. Cross-check 1st-party against
  DefiLlama/Dune/Etherscan-labels to narrow it.
- **I0 is opt-in.** A protocol with no `_deployments.json` gets a visible
  "contract-inventory NOT enforced" WARN — function coverage is still gated per
  known contract, but a missed contract stays invisible. Author the list to close it.
- **ERC standards** (`chain_to_addresses_source` manifests) are out of scope.
- **Proxy / diamond.** A verified ABI may hide implementation functions. The
  snapshot is ground truth only for the surface it exposes; for proxies, snapshot
  the implementation ABI.
- **EIP-712 typehashes** are not in the function-selector ABI, so `signed_structs`
  are hand-listed, not auto-enumerated. The gate only cross-checks them against
  typed-data manifests.
