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
primitive, which is ScopeBall's entire reason to exist. Nothing failed; the miss
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

## Layout

```
surface/<protocol>/<contract>.abi.json        ← snapshot (ground truth)
surface/<protocol>/<contract>.coverage.json   ← triage (decisions)
```

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
  "address": "0x…",                 // must equal the snapshot's
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

## Invariants (any violation → `npm run check:surface` exits 1)

| ID  | Rule | Catches |
|-----|------|---------|
| **I1**  | every snapshot external-mutating selector (`type==="function"` && `stateMutability ∈ {nonpayable,payable}`) has a `functions` entry | the original miss — a function nobody triaged |
| **I1'** | every `functions` key is a real selector that exists in the snapshot | stale / typo'd coverage |
| **I2**  | every `cover` selector has a manifest at `(chain,address,selector)` | triaged-as-cover but adapter never built |
| **I3**  | every on-chain manifest selector at `(chain,address)` is `cover` here | a manifest for a function you marked exclude / never triaged |
| **S1/S2** | each typed-data manifest `primary_type` ↔ a `signed_structs` cover (both ways) | off-chain EIP-712 grant un-triaged |

## Onboarding a new contract (4 steps)

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
   Add off-chain EIP-712 messages under `signed_structs`.
4. **Gate** — `npm run check:surface`. Fix every `✗` until it PASSes. Build the
   `cover` manifests; the gate then proves nothing was dropped.

## Scope & limits (honest)

- **Incremental, never silent.** Only contracts with a snapshot here are
  enforced. Contracts with manifests but no snapshot are reported as a visible
  `UNGATED` WARN (not a failure) — onboard them protocol by protocol.
- **ERC standards** (`chain_to_addresses_source` manifests) are out of scope.
- **Proxy / diamond.** A verified ABI may hide implementation functions. The
  snapshot is ground truth only for the surface it exposes; for proxies, snapshot
  the implementation ABI.
- **EIP-712 typehashes** are not in the function-selector ABI, so `signed_structs`
  are hand-listed, not auto-enumerated. The gate only cross-checks them against
  typed-data manifests.
