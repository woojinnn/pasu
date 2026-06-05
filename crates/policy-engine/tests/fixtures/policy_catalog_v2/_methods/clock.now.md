# method: clock.now

status: existing (in `schema/method-catalog.json`)

> Implementer-facing spec — **HOW** to build `clock.now`, not just its wire shape. Grounded in
> `schema/method-catalog.json` (the authoritative method definition), the two consuming catalog
> manifests/policies, `browser-extension/backend/service-worker/local-method-handlers.ts` (the
> reuse target), and `POLICY_RPC_METHODS.md` (the wire contract). `.md` is gitignored repo-wide, so
> this file is self-contained.

---

## purpose

`clock.now` returns the **current wall-clock time as Unix epoch seconds** (a single `Long`). It
exists so a policy can reason about *how far in the future* a signed expiry sits — the one fact a
static decoder cannot supply, because "now" is not in the calldata/signature. EIP-2612 `permit`
deadlines and intent-order `validUntil` fields are absolute Unix timestamps decoded straight from
the payload; on their own they are just large numbers. Subtracting `now` turns them into a **TTL**
(`deadline - now`), which is what the user actually cares about: a permit valid for >1 year, or an
intent order fillable for >1 week, is a stale-signature / stale-price risk regardless of the
absolute timestamp value. Both consuming policies are `forbid … when { context.validUntil/deadline
> context.custom.nowTs + <window> }` — i.e. they need `nowTs` purely as the reference point for the
window arithmetic. **Unit invariant:** the engine lowers `deadline`/`validUntil` via
`Time::from_unix(...).as_unix()` (see `crates/policy-engine/src/lowering_v2/token/permit2_*.rs`), so
they are Unix *seconds*; `nowTs` **must** be Unix seconds too or the comparison is meaningless.

## interface

**params:** none. (`schema/method-catalog.json` → `"params": {}`; both manifests pass
`"params": {}`.) The method takes no `$.`-selectors — it is a nullary call.

**result shape (record):**

| field | type | meaning |
|---|---|---|
| `nowTs` | Long | current time, Unix epoch **seconds** (UTC). REQUIRED — the only field. |

> Even a single-field result is a JSON record on the wire (`{ "nowTs": 1717372800 }`); the manifest
> projects it down to a scalar leaf (next row). v2 has **no record `ProjectionType`** — `nowTs` would
> never reach `context.custom` un-projected.

**projection (manifest `outputs[].from → type`):**

```
$.result.nowTs  ->  Long        (lands at context.custom.nowTs : Long)
```

Both consuming manifests use exactly:
```json
"outputs": [{ "kind": "context", "field": "nowTs", "type": "Long", "from": "$.result.nowTs", "required": false }],
"custom_context": { "fields": { "nowTs": "Long" } }
```

> **Field-name note (honest):** the task brief referenced a projection `$.result.unixSeconds`. The
> **canonical** field in `schema/method-catalog.json` and in both live catalog manifests is **`nowTs`**
> (`"from": "$.result.nowTs"`). Implement `nowTs`. If a `unixSeconds` alias is ever wanted, emit it
> as an **additional** field alongside `nowTs` (never instead of it) so existing manifests keep
> resolving; do not rename.

## data source(s)

**Host monotonic-of-wall-clock — local, no I/O.** The source is the dispatcher process's own system
clock: `Date.now()` (JS, `ms → /1000`) or `SystemTime::now()` (Rust, `UNIX_EPOCH` elapsed seconds).
There is no external API, no chain read, no key.

- **EXISTING-FETCHER-REUSABLE — but the reuse target is *not* a `sync` fetcher.** The
  `crates/policy-server/sync/src/sources/fetchers/{oracle,onchain,registry,venue}` fetchers are the
  decode-time `live_inputs` layer and all do network I/O — none of them is right for a clock. The
  **correct reuse target is `browser-extension/backend/service-worker/local-method-handlers.ts`**,
  which already short-circuits pure, no-I/O methods (`token.normalize_to_nano`) **before** any
  `/v1/rpc` POST. `clock.now` is the textbook case for that module: its inputs are nothing, its
  output is a pure function of the host clock, and routing it over HTTP would make a user's policy
  depend on the daemon being up for a value the daemon adds nothing to. **Recommendation: implement
  `clock.now` as a `LOCAL_HANDLERS["clock.now"]` entry, mirroring `token.normalize_to_nano`.** It
  then never reaches the `/v1/rpc` server at all.

- **NET-NEW (optional, not required):** an *on-chain* clock (`block.timestamp` of `latest`) would be
  NET-NEW plumbing — `RpcRouter` today exposes `eth_block_number`
  (`crates/policy-server/sync/src/sources/fetchers/rpc/router.rs:144`) but **no** block-timestamp
  read. Do **not** build this for `clock.now`: a per-chain block read adds a network round-trip and a
  chain-id param to a method the catalog deliberately defined as nullary/local, and block timestamps
  lag wall-clock by a block interval. Host wall-clock is the right and sufficient source.

## derivation algorithm

Local handler (the recommended path), pseudocode mirroring `token.normalize_to_nano`:

1. Ignore `params` (nullary). Do not validate/parse — there is nothing to read.
2. Read the host clock as integer seconds:
   - **JS (`local-method-handlers.ts`):** `const nowTs = Math.floor(Date.now() / 1000);`
   - **Rust (`/v1/rpc` server, if not done locally):**
     `let nowTs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;`
3. Sanity-clamp: `nowTs` must be a positive Long that fits JS `Number.MAX_SAFE_INTEGER`
   (2^53−1 ≈ 9.007e15). A Unix-second clock (~1.7e9) is ~6 orders of magnitude under that for
   centuries — no overflow path realistically. Still, follow the module's existing discipline:
   reject `nowTs <= 0` as `local_error` rather than emit a nonsense value.
4. Return `{ "nowTs": nowTs }`.

**Window arithmetic is in the policy, not here.** `clock.now` returns *only* `now`; the
`+ 31536000` (1 year, `permit-far-deadline`) and `+ 604800` (1 week, `intent-order-far-validuntil`)
windows are Cedar literals in the respective `policy.cedar`. Keep this method a pure clock read.

**Honest heuristic limits:** `nowTs` is the *host's* notion of now. (a) If the host clock is skewed
(NTP drift, a manually wrong system clock), the TTL is off by that skew — a few seconds of drift is
immaterial against year/week windows, but a grossly-wrong clock could mis-fire. (b) There is an
inherent race: `now` is sampled at verdict time, not sign/broadcast time; the gap is sub-second and
again immaterial against the windows. (c) Always UTC seconds — never local-tz, never milliseconds.
These are acceptable for a coarse "far-future expiry?" guard and should not be over-engineered into
a chain-anchored clock.

## on-chain calls

**none (host-local clock).** No chain, no contract, no view function, no multicall. (`chain_id` is
not even a param — the method is chain-agnostic.) The on-chain `block.timestamp` alternative is
explicitly declined above as unnecessary NET-NEW plumbing.

## caching / ttl

**Do not cache.** A clock's whole value is freshness; a cached `now` is a stale `now`. Compute it
fresh on every call.

- **key tuple:** n/a (nullary, uncached).
- **ttl:** 0 (recompute every dispatch).
- **where:** computed inline in the handler; nothing stored.
- **HARD_TIMEOUT_MS = 8000 budget:** trivially satisfied — a `Date.now()` / `SystemTime::now()` read
  is sub-microsecond and, via the local-handler path, incurs **zero** network latency (no POST to
  `/v1/rpc` at all). This is the cheapest method in the catalog.

## failure & fallback (DORMANCY CONTRACT)

A host clock read does not realistically fail; but the contract still holds and must be honored:

1. On any error (or if the dispatcher is unreachable / not yet implemented), the handler emits **no
   `nowTs` field** — it returns an `ok:false` result (local handler) or simply isn't served.
2. The host **fold drops** an absent/`ok:false` result: `map[call_id]` gets no `nowTs`.
3. `context.custom` therefore **lacks `nowTs`**.
4. The policy's guard `context.custom has nowTs` evaluates **false**, so the `when`-clause
   short-circuits before the comparison — the `forbid` cannot fire.
5. The policy is **INERT** (dormant): it produces **no verdict at all**, never a false `warn`.

**Never substitute a default.** Do **not** fall back to `0`, to a hardcoded constant, or to any
placeholder `now`: a bogus `nowTs` would let `deadline > nowTs + window` flip a verdict
(`nowTs = 0` makes every deadline look "far-future" → spurious `warn`; a far-future `nowTs` makes
every deadline look fine → missed `warn`). A *missing* field is correct; a *wrong* field is a false
verdict. Both manifests set `"required": false` + top-level `"optional": true`, so a missing `nowTs`
degrades the call to a no-op (the batch **passes**, the policy stays inert) — never a hard
`__system__` batch fail.

## auth / cost / rate-limit

- **API keys:** none. No env var, no credential.
- **per-call cost:** ~0 (one syscall-level clock read; no network).
- **rate-limit:** none — there is no upstream to throttle. Safe to call on every action.
- **caching absorbs cost:** n/a — there is no cost to absorb, and caching would *break* correctness
  (see above). This method is exempt from the caching-for-cost rationale that applies to the
  network-backed methods.

## activation

Implementing `clock.now` un-dormants exactly these two catalog policies (per the activation map in
`POLICY_RPC_METHODS.md` §4):

| Catalog id | path | severity | fires when |
|---|---|---|---|
| `permit-far-deadline` (B2) | `action/approval/permit-far-deadline` | warn | `context.deadline > nowTs + 31536000` (EIP-2612 permit valid >1 year) |
| `intent-order-far-validuntil` (B8) | `protocol/intent-venue/intent-order-far-validuntil` | warn | `context.validUntil > nowTs + 604800` (intent order fillable >1 week) |

Both are `warn`-severity and gated on `context.custom has nowTs`; both pass `params: {}` and project
`$.result.nowTs → Long`. `clock.now` is "existing" in `method-catalog.json`, so the only missing
piece is the dispatcher entry — and since it is pure/local, the cheapest first activation in the
whole catalog (`POLICY_RPC_METHODS.md` §4 names `oracle.usd_value` (5) + `clock.now` (2) as the
minimal first cut).

## primary-source references

- `schema/method-catalog.json` → `"clock.now"` block: `"description": "Current Unix timestamp
  (seconds) from the daemon's clock."`, `"params": {}`, `"returns": { "kind": "scalar", "type":
  "Long", "from": "$.result.nowTs" }`, `"origin": "bundled"`. (Authoritative method definition.)
- Consuming manifests/policies (field name `nowTs`, projection, windows):
  `crates/policy-engine/tests/fixtures/policy_catalog_v2/action/approval/permit-far-deadline/{manifest,policy}.{json,cedar}`
  and `…/protocol/intent-venue/intent-order-far-validuntil/{manifest,policy}.{json,cedar}`.
- Reuse target & local-handler pattern:
  `browser-extension/backend/service-worker/local-method-handlers.ts` (`LOCAL_HANDLERS`,
  `tryHandleLocally`, the `MAX_SAFE` clamp discipline).
- Wire contract / dormancy / record→scalar constraint:
  `browser-extension/backend/service-worker/POLICY_RPC_METHODS.md` §1, §2, §3a, §4.
- Unit (Unix seconds) of `deadline`/`validUntil`:
  `crates/policy-engine/src/lowering_v2/token/permit2_sign_transfer.rs` (`action.sig_deadline.as_unix()`)
  and siblings using `Time::from_unix(...)`.
- EIP-2612 (`permit` deadline semantics) and the EIP-712 typed-order `validUntil` convention are
  standard Unix-epoch-seconds; the absolute spec text for EIP-2612 is at
  https://eips.ethereum.org/EIPS/eip-2612 (§"permit"). Per-venue intent `validUntil` wording is
  venue-specific — **출처 미확인** for an Ethereum-standard citation (it is a UniswapX/CoW-style
  convention, not an EIP), but the *unit* is verified Unix seconds by the engine lowering above.
- Host wall-clock primitives are language stdlib (`Date.now()` MDN / Rust `std::time::SystemTime`);
  no protocol citation applies.
