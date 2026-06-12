# method: address.activity

status: aspirational (referenced; not yet in method-catalog.json — register on implement)

> Implementer-facing build spec for the `/v1/rpc` enrichment method `address.activity`.
> Interface-level description lives in
> `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §3c (`#### address.activity`)
> and §4 (activation map). This file is the **HOW** (derivation, plumbing, caching, failure
> contract); that file is the **WHAT** (wire shape). They must stay consistent — if you change a
> param/result name here, change it there too.

## purpose

정책이 "수령 주소가 얼마나 established 한가"를 판단하려면, 정적 calldata 만으로는 알 수 없는
사실(해당 주소가 체인에서 실제로 활동한 적이 있는가)이 필요하다. `address.activity` 는 한 주소의
체인 활동성을 요약한다: 보낸 트랜잭션 수(`txCount`, = nonce), EOA/contract 구분(`isContract`),
그리고 (가능하면) 최초 관측 시각(`firstSeenTs`). 1차 소비자는 카탈로그 정책
`transfer-new-recipient` (action `transfer`) 으로, **`txCount == 0` 인 갓-생성 주소로의 전송을 `warn`**
시킨다 — address-poisoning / typo-주소 / 갓-배포 drainer 컨트랙트의 전형적 신호다. Dambi 의
no-simulation 모델과 일관되게 이 메서드는 트랜잭션 트레이스가 아니라 **사실 1건을 fetch** 하는 호출이다.

## interface

(Source of truth for the wire shape: `POLICY_RPC_METHODS.md` §3c. Reproduced here; do not diverge.)

### params (`$.`-selectors)

| param | selector | type | note |
|---|---|---|---|
| `chain_id` | `$.root.chain_id` | Long | EIP-155 chain id (e.g. `1`). Selects which RPC pool to hit. |
| `address` | `$.action.recipient` | String | The recipient/spender address under review (0x-hex, 20 bytes). For `transfer-new-recipient` this is the transfer recipient; the planner is authority for the exact ActionView spelling (`$.action.recipient` vs `$.action.to`) — see §2/§3a "Note on selectors". |

`call_id = "<manifest_id>::<spec_id>"` per §1 wire contract.

### result shape (record)

```json
{ "txCount": <Long>, "isContract": <Bool>, "firstSeenTs": <Long|absent> }
```

| field | type | required | meaning |
|---|---|---|---|
| `txCount` | Long | yes | Outbound transaction count = account nonce at `latest`. `0` ⇒ never sent a tx. |
| `isContract` | Bool | yes | `true` iff deployed code at the address is non-empty. |
| `firstSeenTs` | Long | **optional** | Unix seconds of first on-chain appearance. Omitted when no indexer is wired (see derivation). Absence is legal — projecting it is for a *future* policy only. |

### projection (record → scalar leaf — MANDATORY)

v2 `materialize_v2` admits only scalar projection types (`String | Long | Bool | Decimal | Set<String>`);
there is **no record ProjectionType**. The manifest MUST project the record to scalar leaves via
`outputs[].from`:

| `outputs[].from` | `outputs[].type` | `custom_context` field (lowercase Cedar) | consumer |
|---|---|---|---|
| `$.result.txCount` | `Long` | `Long` | `transfer-new-recipient` (this is THE leaf it tests) |
| `$.result.isContract` | `Bool` | `Bool` | future contract-recipient policy |
| `$.result.firstSeenTs` | `Long` | `Long` | future "fresh address by age" policy |

The primary/required projection is `$.result.txCount → Long`. `outputs[].field ⇄ custom_context.fields`
is 1:1 (enforced by `ManifestV2::validate`).

## data source(s)

Two-tier. Tier-1 is sufficient to activate `transfer-new-recipient`; Tier-2 is optional uplift.

- **Tier-1 — chain JSON-RPC (`txCount` + `isContract`)** — *NET-NEW plumbing, but small, on existing rails.*
  - `txCount` ← `eth_getTransactionCount(address, "latest")`.
  - `isContract` ← `eth_getCode(address, "latest")` then `code != "0x"`.
  - **Reuse:** `crates/policy-server/sync/src/sources/fetchers/rpc/RpcRouter` already provides
    per-chain provider routing, failover (`try_all`), and a generic
    `RpcProvider::call_method<T>(method, params)` helper
    (`rpc/providers/public.rs:32`). `eth_call` / `eth_balance` / `eth_blockNumber` are wired through
    it the same way; `eth_getTransactionCount` and `eth_getCode` are **not yet exposed** on the
    `RpcRouter`/`RpcProvider` surface, so each needs a thin wrapper (mirror `eth_balance` at
    `public.rs:117` / `router.rs:131`) — a few lines each, no new transport.
  - **Reuse:** `OnchainViewFetcher` (`onchain.rs`) shows the batching pattern — `fetch_batch` packs
    multiple reads into one `Multicall.aggregate3`. `eth_getTransactionCount`/`eth_getCode` are
    *account-state* RPCs (not `eth_call`), so they cannot ride Multicall3; batch them instead by
    issuing the two JSON-RPC calls concurrently (`tokio::join!`) within the one method handler.

- **Tier-2 — `firstSeenTs` (OPTIONAL, NET-NEW)** — an external indexer/explorer:
  the earliest tx timestamp for an address is **not** obtainable from a stock JSON-RPC node (no
  `eth_*` returns it). It requires an explorer API (Etherscan `account?action=txlist&sort=asc&page=1&offset=1`
  → `result[0].timeStamp`) or an indexer. **No such fetcher exists in-repo** (`fetchers/` has
  `oracle`, `onchain`, `registry`, `venue`, `rpc`, `abi_decoder` — none index account history).
  Until one is added, **omit `firstSeenTs` entirely** (see DORMANCY CONTRACT) — Tier-1 alone fully
  activates the only current consumer.

## derivation algorithm

1. Parse `chain_id` (Long) → `ChainId`; reject if no RPC pool configured for it (→ failure path).
2. Normalize `address` to lowercase 0x-hex; validate 20-byte length (→ failure path on malformed).
3. **Tier-1 fetch (concurrent):**
   - `nonce_hex = eth_getTransactionCount(address, "latest")` → parse hex → `txCount: u64`.
   - `code_hex  = eth_getCode(address, "latest")` → `isContract = code_hex != "0x" && code_hex != "0x0"`.
   - Issue both via `tokio::join!` against the same chain's `RpcRouter` (one round-trip latency, not two).
4. **Tier-2 (only if an indexer fetcher is wired):** query earliest-tx timestamp; on success set
   `firstSeenTs`. On any error/absence, **do not** set the field (leave it out of the record).
5. Assemble `{ txCount, isContract, [firstSeenTs] }` and return as the unwrapped `$.result` payload.

**Heuristic limits (honest):**
- `txCount` = **outbound** nonce only. A funded-but-never-spent address, or a smart-contract
  account that receives via internal calls / `CREATE2` counterfactual, can show `txCount == 0` while
  still being "real". This is acceptable for `transfer-new-recipient`'s **warn** (not deny) intent:
  the policy raises a *caution*, the user confirms. It is **not** a liveness proof.
- `isContract == false` does not prove an address is a safe EOA (could be a not-yet-deployed CREATE2
  target). Treat it as a hint, never as a hard gate.
- `firstSeenTs` is best-effort and indexer-dependent; never block on its absence.

## on-chain calls

These are **account-state JSON-RPC reads** (not `eth_call` to a contract view fn), so there is no
contract/function/decoder and **no Multicall3** (Multicall3 can only aggregate `eth_call`):

| RPC | chain | params | block |
|---|---|---|---|
| `eth_getTransactionCount` | `eip155:<chain_id>` (from `$.root.chain_id`) | `[address, "latest"]` | latest |
| `eth_getCode` | same | `[address, "latest"]` | latest |

Both go through the existing `RpcRouter` provider pool (failover-capable). Tier-2 `firstSeenTs` is an
**off-chain data-API** call, not on-chain.

## caching / ttl

- **Key tuple:** `(chain_id, address_lowercase)`.
- **TTL:** Tier-1 `txCount`/`isContract` ~ **60–120 s** (nonce/code change rarely within a signing
  session; a fresh recipient stays fresh). Tier-2 `firstSeenTs` is effectively immutable once seen →
  cache **long / indefinitely** (only ever set once per address).
- **Where:** in the `/v1/rpc` dispatcher's per-method result cache (host process, in-memory; same
  layer the dispatcher uses for `oracle.*`/`portfolio.*`). Keyed independently of `request_id` so
  repeat actions to the same recipient are free.
- **Budget:** must fit `HARD_TIMEOUT_MS = 8000` (whole-action budget, shared across *all* planned
  calls in the batch). Two concurrent JSON-RPC reads (`tokio::join!`) to a single chain are ~1 RTT
  (typically <300 ms warm, <1 s cold). On cache hit: ~0 ms. The handler MUST itself time out well
  under the global budget (suggest a per-method deadline of ~2 s) and, on its own timeout, take the
  failure path (emit no field) rather than stalling the batch.

## failure & fallback (DORMANCY CONTRACT)

This is the load-bearing invariant — **a missing fact must never flip a verdict.**

- On **any** error or missing data (no RPC pool for chain, malformed address, RPC error/timeout,
  provider failover exhausted, Tier-2 indexer absent): **emit NO field** for that leaf
  (`ok:false` for the call, or simply omit the optional field from the record).
- Host fold (`POLICY_RPC_METHODS.md` §1): a missing/`ok:false` result ⇒ `map[call_id]` lacks the
  value ⇒ `context.custom` lacks `txCount` ⇒ the policy's `context.custom has txCount` guard is
  **false** ⇒ `transfer-new-recipient` is **INERT** (no verdict at all) — never a false `warn`,
  never a false `pass`.
- **NEVER substitute a default.** Returning `txCount: 0` on a fetch error would *fabricate* a
  "fresh recipient" and produce a spurious `warn`; returning a large number would *suppress* a real
  warning. Both are forbidden — absence is the only honest signal of "couldn't determine".
- Catalog policies declare this call **`optional: true`** (every catalog enrichment does), so a
  missing `address.activity` **degrades to `pass`** for that policy and **never hard-fails the
  batch**. A dormant or unreachable `/v1/rpc` dispatcher is therefore safe by construction.
- `firstSeenTs` is independently optional within a *successful* call: Tier-1 can succeed and still
  omit `firstSeenTs` with no effect on `transfer-new-recipient` (which only reads `txCount`).

## auth / cost / rate-limit

- **Tier-1 (JSON-RPC):** no API key beyond whatever the configured RPC provider pool already uses
  (`rpc/config.rs` / `RpcConfig`; public endpoints or keyed providers per env). Cost = 2 light
  account-state reads per *uncached* (chain, address). Public-node rate limits are the main concern;
  the **60–120 s cache** absorbs repeats, and a single signing session touches few distinct
  recipients, so steady-state QPS is low. Failover across the provider pool (`try_all`) already
  spreads load.
- **Tier-2 (`firstSeenTs`):** if implemented via Etherscan-style API, needs an `ETHERSCAN_API_KEY`
  (env) and is rate-limited (free tier ~5 req/s). The **indefinite cache** (immutable value) makes
  this near-zero steady-state cost. If no key/indexer is provisioned, ship Tier-1 only.
- All keys come from server env, never from the manifest or the extension.

## activation

Implementing this method (Tier-1 alone suffices) un-dormants these catalog policies:

- **`transfer-new-recipient`** (action `transfer`) — `warn` when `context.custom.txCount == 0`.

(Per `POLICY_RPC_METHODS.md` §4 activation map: `address.activity` → 1 policy.) Registering the
method in `schema/method-catalog.json` is part of "implement" — until then the policy compiles but
stays dormant.

## primary-source references

- **`eth_getTransactionCount`** — Ethereum JSON-RPC API specification (returns the number of
  transactions sent from an address; this is the account nonce). Official spec:
  https://ethereum.org/en/developers/docs/apis/json-rpc/#eth_gettransactioncount and the
  machine-readable `ethereum/execution-apis` (`eth_getTransactionCount`).
- **`eth_getCode`** — Ethereum JSON-RPC API specification (returns code at a given address; empty
  `0x` ⇒ EOA / undeployed). https://ethereum.org/en/developers/docs/apis/json-rpc/#eth_getcode and
  `ethereum/execution-apis` (`eth_getCode`).
- **EIP-155** — chain id binding (the `chain_id` param's namespace).
  https://eips.ethereum.org/EIPS/eip-155.
- **In-repo plumbing (verify against code, not this doc):** `RpcRouter` /
  `RpcProvider::call_method` — `crates/policy-server/sync/src/sources/fetchers/rpc/{router.rs,providers/public.rs}`;
  batching/eth_call pattern — `.../fetchers/onchain.rs` (`OnchainViewFetcher::fetch_batch`);
  wire contract / projection / fold / dormancy — `POLICY_RPC_METHODS.md` §§1–3.
- **Tier-2 `firstSeenTs` via explorer** (account txlist earliest record): Etherscan API docs —
  https://docs.etherscan.io/ (`account` module, `txlist` action). 출처 미확인 — that any *specific*
  provider/free-tier limit applies to your deployment; confirm against the provider you provision.
- The claim that no stock `eth_*` JSON-RPC method returns first-seen timestamp: 출처 미확인 as a
  cited negative, but consistent with the `execution-apis` method set (no account-history method
  exists there).
