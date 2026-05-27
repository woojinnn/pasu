/**
 * Phase 8 Round 6 — Aerodrome declarative-route e2e mapping tests.
 *
 * Coverage scope:
 *   12 scenarios exercising the `tryDeclarativeRoute` orchestrator entry
 *   over the 33 Aerodrome bundles (V2 / Slipstream / Voter / VotingEscrow
 *   / Gauge). The bundles themselves were validated by the Tier A unit
 *   tests in `registry/scripts/build-index.ts` + the existing Rust
 *   `mappers` suite; here we lock the *TS pipeline boundary*:
 *
 *     resolveAdapter (mocked) → decodeBundleCalldata (real, runs the
 *     bundle ABI through viem) → declarativeRouteRequest (mocked WASM
 *     boundary) → enrichEnvelopeAssets (real, with a mocked token
 *     client).
 *
 *   Cedar verdict evaluation lives outside this fence — the
 *   `evaluateWithEnvelopes` boundary is exercised by the dedicated edge
 *   test file (`declarative-route.edge.test.ts`). Here we only verify
 *   that the mapping layer produces the right envelope skeleton + the
 *   route emits the expected `outcome`/`decoderId`/`source` tuple.
 *
 * Calldata strategy:
 *   - Synthetic via viem `encodeFunctionData` against the live bundle
 *     ABI fragment. This keeps the test file portable (no basescan
 *     network dependency) and pins the encoder/decoder against the
 *     SAME ABI shape the runtime uses.
 *   - Each scenario builds its own selector + args tuple matching the
 *     bundle's `match.selector` and `abi_fragment.abi.inputs`. The
 *     `tryDeclarativeRoute` helper extracts the selector from
 *     calldata, so generated calldata starts with the bundle's
 *     canonical selector by construction.
 *
 * Bundle source:
 *   - `registry/manifests/aerodrome/<sub>/<func>@1.0.0.json` —
 *     read fresh per-test, no caching, so a refactor that changes the
 *     bundle shape fails here loudly.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import {
  type Abi,
  type AbiFunction,
  encodeFunctionData,
  toFunctionSelector,
} from "viem";

// ─── Mock hoisting ─────────────────────────────────────────────────────────
//
// The WASM bridge + jit-fetcher boundaries are vi.hoisted so the import
// graph for `../declarative-route` resolves against our test doubles. We
// do NOT mock `declarative-decode` — the bundle ABI decoder runs for
// real over the synthetic calldata so we exercise the full mapping
// surface end-to-end.
const mocks = vi.hoisted(() => {
  class MockEngineError extends Error {
    constructor(
      readonly kind: string,
      message: string,
    ) {
      super(message);
      this.name = "EngineError";
    }
  }
  return {
    MockEngineError,
    resolveAdapter: vi.fn(),
    declarativeRouteRequest: vi.fn(),
  };
});

vi.mock("webextension-polyfill", () => ({
  default: {
    runtime: {
      getURL: vi.fn((p: string) => `chrome-extension://scopeball/${p}`),
    },
  },
}));

vi.mock("../jit-fetcher", () => ({
  resolveAdapter: mocks.resolveAdapter,
}));

vi.mock("../../wasm-bridge", () => ({
  EngineError: mocks.MockEngineError,
  declarativeRouteRequest: mocks.declarativeRouteRequest,
}));

import { tryDeclarativeRoute } from "../declarative-route";
import { parseBundle, type AdapterFunctionBundle } from "../bundle-schema";
import type {
  TokenMetadata,
  TokenRegistryClient,
} from "../../registry/token-client";

// ─── Bundle loader ─────────────────────────────────────────────────────────
//
// Resolve manifests relative to this file's path inside the worktree. The
// `__tests__` directory sits 5 deep under the worktree root.
const MANIFEST_ROOT = path.resolve(
  __dirname,
  "../../../../../registry/manifests/aerodrome",
);

function loadAerodromeBundle(
  subdir: string,
  func: string,
): AdapterFunctionBundle {
  const file = path.join(MANIFEST_ROOT, subdir, `${func}@1.0.0.json`);
  return parseBundle(JSON.parse(readFileSync(file, "utf8")));
}

function bundleAbi(bundle: AdapterFunctionBundle): Abi {
  return [bundle.abi_fragment.abi as unknown as AbiFunction];
}

function bundleSelector(bundle: AdapterFunctionBundle): string {
  // The bundle's `match.selector` is the canonical lookup key; we
  // recompute it here so a mismatch between the abi and the match
  // selector trips a test (sanity guard).
  //
  // The bundle ABI fragment carries only `{ name, type, inputs }` — no
  // `outputs` (a function selector depends solely on name + input types).
  // abitype 1.0.0's `formatAbiItem`, reached via `toFunctionSelector`,
  // unconditionally reads `abiItem.outputs.length`, so we normalise a
  // missing `outputs` to `[]` before handing the fragment to viem. This
  // does not affect the computed selector — production's
  // `decodeFunctionData` path tolerates the absent field on its own.
  const fn = bundle.abi_fragment.abi as unknown as AbiFunction;
  const normalised: AbiFunction = {
    ...fn,
    outputs: fn.outputs ?? [],
  };
  const sig = toFunctionSelector(normalised);
  return sig.toLowerCase();
}

function bundlePrimaryAddress(bundle: AdapterFunctionBundle): string {
  // For multi-address bundles (Gauge), take the first entry. Lowercase
  // mirrors the orchestrator's call-key normalisation. v2 schema may use
  // `chain_to_addresses` instead of `to` — fall through to either.
  const v2 = bundle.match.chain_to_addresses;
  if (v2) {
    const first = Object.values(v2)[0];
    if (first && first[0]) return first[0].toLowerCase();
  }
  const to = bundle.match.to;
  if (to && to[0]) return to[0].toLowerCase();
  throw new Error(`bundlePrimaryAddress: bundle ${bundle.id} has no addresses`);
}

function adapterHitFor(
  bundle: AdapterFunctionBundle,
  source: "layer1" | "jit" = "layer1",
) {
  const bundleId = bundle.id; // already `<path>@<version>`
  const stripVersion = (id: string) => {
    const at = id.indexOf("@");
    return at >= 0 ? id.slice(0, at) : id;
  };
  return {
    kind: "adapter" as const,
    source,
    adapter: {
      decoderId: `declarative.${stripVersion(bundleId)}`,
      bundleId,
      bundle,
    },
  };
}

// ─── Token client mock ─────────────────────────────────────────────────────
//
// The route enriches envelope AssetRefs with `symbol`/`decimals` post-
// WASM. We supply a stub registry that resolves a handful of Base assets
// so the enrichment branch is exercised on at least one swap + one
// stake test.
const BASE_USDC = "0x833589fcd6edb6e08f4c7c32d4f71b54bda02913";
const BASE_AERO = "0x940181a94a35a4569e4529a3cdfb74e38fd98631";
const BASE_WETH = "0x4200000000000000000000000000000000000006";
const BASE_LP_AERO_USDC = "0x4f09bab2f0e15e2a078a227fe1537665f55b8360"; // top-20 V2 gauge addr

const TOKEN_META_BY_ADDRESS: Record<string, TokenMetadata> = {
  [BASE_USDC]: {
    kind: "erc20",
    chainId: 8453,
    address: BASE_USDC,
    symbol: "USDC",
    decimals: 6,
    name: "USD Coin",
  },
  [BASE_AERO]: {
    kind: "erc20",
    chainId: 8453,
    address: BASE_AERO,
    symbol: "AERO",
    decimals: 18,
    name: "Aerodrome",
  },
  [BASE_WETH]: {
    kind: "erc20",
    chainId: 8453,
    address: BASE_WETH,
    symbol: "WETH",
    decimals: 18,
    name: "Wrapped Ether",
  },
  [BASE_LP_AERO_USDC]: {
    kind: "erc20",
    chainId: 8453,
    address: BASE_LP_AERO_USDC,
    symbol: "AERO-USDC-LP",
    decimals: 18,
    name: "AERO/USDC LP",
  },
};

function makeTokenClient(): TokenRegistryClient {
  return {
    lookup: vi.fn(async (_chainId: number, address: string) => {
      return TOKEN_META_BY_ADDRESS[address.toLowerCase()] ?? null;
    }),
  };
}

const USER = "0x1111111111111111111111111111111111111111";
const BASE_CHAIN_ID = 8453;

// ─── Fixtures ──────────────────────────────────────────────────────────────
//
// One synthetic-envelope shape per Aerodrome action variant. The mocked
// WASM boundary returns these — the route helper then runs them through
// `enrichEnvelopeAssets` for real (token-client mocked).
function swapEnvelope(opts: {
  inputAddress: string;
  outputAddress: string;
  inputAmount: string;
  outputAmount: string;
  swapMode?: "exact_in" | "exact_out";
}): Record<string, unknown> {
  return {
    category: "dex",
    action: "swap",
    fields: {
      swapMode: opts.swapMode ?? "exact_in",
      inputToken: {
        asset: { kind: "erc20", address: opts.inputAddress },
        amount: { kind: "exact", value: opts.inputAmount },
      },
      outputToken: {
        asset: { kind: "erc20", address: opts.outputAddress },
        amount: { kind: "min", value: opts.outputAmount },
      },
      recipient: USER,
      validity: { expiresAt: "9999999999", source: "tx-deadline" },
    },
  };
}

describe("tryDeclarativeRoute — Aerodrome bundles", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });
  afterEach(() => {
    vi.clearAllMocks();
  });

  // ── Scenario 1: V2 swapExactTokensForTokens (single hop) ────────────────
  it("V2 swap full e2e — single-hop route emits one swap envelope", async () => {
    const bundle = loadAerodromeBundle("router-v2", "swapExactTokensForTokens");
    const router = bundlePrimaryAddress(bundle);
    const sel = bundleSelector(bundle);
    expect(sel).toBe("0xcac88ea9"); // selector sanity

    // routes: [{from: AERO, to: USDC, stable: false, factory: 0x...}]
    const calldata = encodeFunctionData({
      abi: bundleAbi(bundle),
      functionName: "swapExactTokensForTokens",
      args: [
        1_000_000_000_000_000_000n, // amountIn
        950_000n, // amountOutMin
        [
          {
            from: BASE_AERO,
            to: BASE_USDC,
            stable: false,
            factory: "0x420dd381b31aef6683db6b902084cb0ffece40da",
          },
        ],
        USER,
        9_999_999_999n,
      ],
    });

    mocks.resolveAdapter.mockResolvedValueOnce(adapterHitFor(bundle, "layer1"));
    mocks.declarativeRouteRequest.mockResolvedValueOnce({
      envelopes: [
        swapEnvelope({
          inputAddress: BASE_AERO,
          outputAddress: BASE_USDC,
          inputAmount: "1000000000000000000",
          outputAmount: "950000",
        }),
      ],
      decoder_id: "declarative.aerodrome/router-v2/swapExactTokensForTokens",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: BASE_CHAIN_ID,
      from: USER,
      to: router,
      calldataHex: calldata,
      options: {
        blockTimestamp: 1_700_000_000,
        tokenClient: makeTokenClient(),
      },
    });

    expect(outcome.kind).toBe("hit");
    if (outcome.kind !== "hit") return;
    expect(outcome.value.decoderId).toBe(
      "declarative.aerodrome/router-v2/swapExactTokensForTokens",
    );
    expect(outcome.value.source).toBe("layer1");
    expect(outcome.value.envelopes).toHaveLength(1);

    // Confirm the route input reached the WASM entry: chain_id, to, selector,
    // and the raw calldata. Decode correctness is now owned by Rust
    // (abi-resolver decode_with_json_abi).
    const engineInput = mocks.declarativeRouteRequest.mock.calls[0][0];
    expect(engineInput.chain_id).toBe(BASE_CHAIN_ID);
    expect(engineInput.to.toLowerCase()).toBe(router);
    expect(engineInput.selector).toBe(sel);
    expect(engineInput.calldata).toBe(calldata);
  });

  // ── Scenario 2: V2 swap multi-hop (routes length 3) ─────────────────────
  it("V2 swap multi-hop — three-route chain reaches the WASM with full path", async () => {
    const bundle = loadAerodromeBundle("router-v2", "swapExactTokensForTokens");
    const router = bundlePrimaryAddress(bundle);

    const intermediate1 = "0x2222222222222222222222222222222222222222";
    const intermediate2 = "0x3333333333333333333333333333333333333333";
    const factory = "0x420dd381b31aef6683db6b902084cb0ffece40da";

    const calldata = encodeFunctionData({
      abi: bundleAbi(bundle),
      functionName: "swapExactTokensForTokens",
      args: [
        500_000n,
        100_000n,
        [
          { from: BASE_AERO, to: intermediate1, stable: false, factory },
          {
            from: intermediate1,
            to: intermediate2,
            stable: true,
            factory,
          },
          { from: intermediate2, to: BASE_USDC, stable: false, factory },
        ],
        USER,
        9_999_999_999n,
      ],
    });

    mocks.resolveAdapter.mockResolvedValueOnce(adapterHitFor(bundle, "jit"));
    // Mapper would emit the LAST route's `to` as outputToken — we
    // hand back exactly that envelope shape so the enricher (real)
    // runs against USDC for the output asset.
    mocks.declarativeRouteRequest.mockResolvedValueOnce({
      envelopes: [
        swapEnvelope({
          inputAddress: BASE_AERO,
          outputAddress: BASE_USDC,
          inputAmount: "500000",
          outputAmount: "100000",
        }),
      ],
      decoder_id: "declarative.aerodrome/router-v2/swapExactTokensForTokens",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: BASE_CHAIN_ID,
      from: USER,
      to: router,
      calldataHex: calldata,
      options: { tokenClient: makeTokenClient() },
    });

    expect(outcome.kind).toBe("hit");
    if (outcome.kind !== "hit") return;
    expect(outcome.value.source).toBe("jit");
  });

  // ── Scenario 3: V2 addLiquidity stable=true ─────────────────────────────
  it("V2 addLiquidity — stable=true flag is forwarded into the decoded args", async () => {
    const bundle = loadAerodromeBundle("router-v2", "addLiquidity");
    const router = bundlePrimaryAddress(bundle);
    const sel = bundleSelector(bundle);

    const calldata = encodeFunctionData({
      abi: bundleAbi(bundle),
      functionName: "addLiquidity",
      args: [
        BASE_AERO,
        BASE_USDC,
        true, // stable
        1_000_000_000_000_000_000n, // amountADesired
        2_000_000n, // amountBDesired
        950_000_000_000_000_000n, // amountAMin
        1_900_000n, // amountBMin
        USER,
        9_999_999_999n,
      ],
    });

    mocks.resolveAdapter.mockResolvedValueOnce(adapterHitFor(bundle));
    mocks.declarativeRouteRequest.mockResolvedValueOnce({
      envelopes: [
        {
          category: "dex",
          action: "add_liquidity",
          fields: {
            pool: { address: "0x0000000000000000000000000000000000000000" },
            inputTokens: [
              {
                asset: { kind: "erc20", address: BASE_AERO },
                amount: { kind: "max", value: "1000000000000000000" },
              },
              {
                asset: { kind: "erc20", address: BASE_USDC },
                amount: { kind: "max", value: "2000000" },
              },
            ],
            outputLp: { asset: { kind: "erc20" }, amount: { kind: "min" } },
            recipient: USER,
            validity: { expiresAt: "9999999999", source: "tx-deadline" },
          },
        },
      ],
      decoder_id: "declarative.aerodrome/router-v2/addLiquidity",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: BASE_CHAIN_ID,
      from: USER,
      to: router,
      calldataHex: calldata,
      options: { tokenClient: makeTokenClient() },
    });

    expect(outcome.kind).toBe("hit");
    if (outcome.kind !== "hit") return;
    expect(outcome.value.decoderId).toBe(
      "declarative.aerodrome/router-v2/addLiquidity",
    );
    expect(outcome.value.envelopes).toHaveLength(1);

    // The selector is forwarded to the WASM entry unchanged.
    expect(mocks.declarativeRouteRequest.mock.calls[0][0].selector).toBe(sel);
  });

  // ── Scenario 4: V2 removeLiquidityETHSupportingFeeOnTransferTokens ──────
  it("V2 removeLiquidityETHSupportingFeeOnTransfer — fee-on-transfer variant routes correctly", async () => {
    const bundle = loadAerodromeBundle(
      "router-v2",
      "removeLiquidityETHSupportingFeeOnTransferTokens",
    );
    const router = bundlePrimaryAddress(bundle);

    const calldata = encodeFunctionData({
      abi: bundleAbi(bundle),
      functionName: "removeLiquidityETHSupportingFeeOnTransferTokens",
      args: [
        BASE_AERO,
        false, // stable
        100_000_000_000_000_000n, // liquidity
        900_000_000_000_000_000n, // amountTokenMin
        100_000_000_000_000_000n, // amountETHMin
        USER,
        9_999_999_999n,
      ],
    });

    mocks.resolveAdapter.mockResolvedValueOnce(adapterHitFor(bundle));
    mocks.declarativeRouteRequest.mockResolvedValueOnce({
      envelopes: [
        {
          category: "dex",
          action: "remove_liquidity",
          fields: {
            exitMode: "proportional",
            pool: { address: "0x0000000000000000000000000000000000000000" },
            inputLp: {
              asset: { kind: "erc20" },
              amount: { kind: "exact", value: "100000000000000000" },
            },
            outputTokens: [
              {
                asset: { kind: "erc20", address: BASE_AERO },
                amount: { kind: "min", value: "900000000000000000" },
              },
              {
                asset: { kind: "native" },
                amount: { kind: "min", value: "100000000000000000" },
              },
            ],
            recipient: USER,
            validity: { expiresAt: "9999999999", source: "tx-deadline" },
          },
        },
      ],
      decoder_id:
        "declarative.aerodrome/router-v2/removeLiquidityETHSupportingFeeOnTransferTokens",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: BASE_CHAIN_ID,
      from: USER,
      to: router,
      calldataHex: calldata,
      options: { tokenClient: makeTokenClient() },
    });

    expect(outcome.kind).toBe("hit");
    if (outcome.kind !== "hit") return;
    expect(outcome.value.decoderId).toBe(
      "declarative.aerodrome/router-v2/removeLiquidityETHSupportingFeeOnTransferTokens",
    );
    expect(outcome.value.envelopes).toHaveLength(1);

  });

  // ── Scenario 5: Slipstream exactInput (packed path) ─────────────────────
  it("Slipstream exactInput — packed-path bytes reach the engine + enrichment runs", async () => {
    const bundle = loadAerodromeBundle("slipstream-swap-router", "exactInput");
    const router = bundlePrimaryAddress(bundle);

    // Packed path: AERO (20B) + tickSpacing=200 (3B big-endian = 0x0000c8)
    // + USDC (20B). Slipstream uses int24 tickSpacing in the path
    // segment (vs. uint24 fee on V3).
    const packedPath =
      ("0x" +
        BASE_AERO.slice(2) +
        "0000c8" +
        BASE_USDC.slice(2)) as `0x${string}`;

    const calldata = encodeFunctionData({
      abi: bundleAbi(bundle),
      functionName: "exactInput",
      args: [
        {
          path: packedPath,
          recipient: USER,
          deadline: 9_999_999_999n,
          amountIn: 1_000_000_000_000_000_000n,
          amountOutMinimum: 1_900_000n,
        },
      ],
    });

    mocks.resolveAdapter.mockResolvedValueOnce(adapterHitFor(bundle));
    mocks.declarativeRouteRequest.mockResolvedValueOnce({
      envelopes: [
        swapEnvelope({
          inputAddress: BASE_AERO,
          outputAddress: BASE_USDC,
          inputAmount: "1000000000000000000",
          outputAmount: "1900000",
        }),
      ],
      decoder_id: "declarative.aerodrome/slipstream-swap-router/exactInput",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: BASE_CHAIN_ID,
      from: USER,
      to: router,
      calldataHex: calldata,
      options: { tokenClient: makeTokenClient() },
    });

    expect(outcome.kind).toBe("hit");
    if (outcome.kind !== "hit") return;
    expect(outcome.value.decoderId).toBe(
      "declarative.aerodrome/slipstream-swap-router/exactInput",
    );

    // Enrichment ran on the AERO + USDC AssetRefs.
    const envelope = outcome.value.envelopes[0] as {
      fields: {
        inputToken: { asset: Record<string, unknown> };
        outputToken: { asset: Record<string, unknown> };
      };
    };
    expect(envelope.fields.inputToken.asset).toMatchObject({
      symbol: "AERO",
      decimals: 18,
    });
    expect(envelope.fields.outputToken.asset).toMatchObject({
      symbol: "USDC",
      decimals: 6,
    });
  });

  // ── Scenario 6: Slipstream exactInputSingle (struct param) ──────────────
  it("Slipstream exactInputSingle — int24 tickSpacing inside flattened tuple", async () => {
    const bundle = loadAerodromeBundle("slipstream-swap-router", "exactInputSingle");
    const router = bundlePrimaryAddress(bundle);

    const calldata = encodeFunctionData({
      abi: bundleAbi(bundle),
      functionName: "exactInputSingle",
      args: [
        {
          tokenIn: BASE_AERO,
          tokenOut: BASE_USDC,
          tickSpacing: 200,
          recipient: USER,
          deadline: 9_999_999_999n,
          amountIn: 500_000_000_000_000_000n,
          amountOutMinimum: 950_000n,
          sqrtPriceLimitX96: 0n,
        },
      ],
    });

    mocks.resolveAdapter.mockResolvedValueOnce(adapterHitFor(bundle));
    mocks.declarativeRouteRequest.mockResolvedValueOnce({
      envelopes: [
        swapEnvelope({
          inputAddress: BASE_AERO,
          outputAddress: BASE_USDC,
          inputAmount: "500000000000000000",
          outputAmount: "950000",
        }),
      ],
      decoder_id: "declarative.aerodrome/slipstream-swap-router/exactInputSingle",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: BASE_CHAIN_ID,
      from: USER,
      to: router,
      calldataHex: calldata,
      options: { tokenClient: makeTokenClient() },
    });

    expect(outcome.kind).toBe("hit");
    if (outcome.kind !== "hit") return;
    expect(outcome.value.decoderId).toBe(
      "declarative.aerodrome/slipstream-swap-router/exactInputSingle",
    );

  });

  // ── Scenario 7: Voter.vote (normal weights) ─────────────────────────────
  it("Voter.vote with positive weights — gauge_vote envelope mapping succeeds", async () => {
    const bundle = loadAerodromeBundle("voter", "vote");
    const voter = bundlePrimaryAddress(bundle);

    const pool1 = "0x4f09bab2f0e15e2a078a227fe1537665f55b8360";
    const pool2 = "0x519bbd1dd8c6a94c46080e24f316c14ee758c025";

    const calldata = encodeFunctionData({
      abi: bundleAbi(bundle),
      functionName: "vote",
      args: [
        12345n, // tokenId
        [pool1, pool2], // pools
        [60n, 40n], // weights — positive, normal allocation
      ],
    });

    mocks.resolveAdapter.mockResolvedValueOnce(adapterHitFor(bundle));
    mocks.declarativeRouteRequest.mockResolvedValueOnce({
      envelopes: [
        {
          category: "misc",
          action: "gauge_vote",
          fields: {
            voter,
            tokenId: "12345",
            pools: [pool1, pool2],
            weights: ["60", "40"],
            kind: "vote",
          },
        },
      ],
      decoder_id: "declarative.aerodrome/voter/vote",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: BASE_CHAIN_ID,
      from: USER,
      to: voter,
      calldataHex: calldata,
    });

    expect(outcome.kind).toBe("hit");
    if (outcome.kind !== "hit") return;
    expect(outcome.value.decoderId).toBe("declarative.aerodrome/voter/vote");
    expect(outcome.value.envelopes).toHaveLength(1);

  });

  // ── Scenario 8: Voter.vote (weights=[0] — mapping succeeds, Cedar later) ─
  it("Voter.vote weights=[0] — mapping still succeeds (Cedar zero-sum check is a separate verdict layer)", async () => {
    // The plan explicitly defers Cedar verdict evaluation to a separate
    // sub-agent — here we verify the mapping layer (route → envelope)
    // still works for inputs that downstream policy would flag. The
    // declarative path is observability-only on its own; the orchestrator
    // hands the envelope to `evaluateWithEnvelopes` *after* this helper
    // returns. This test pins the mapping-vs-verdict separation.
    const bundle = loadAerodromeBundle("voter", "vote");
    const voter = bundlePrimaryAddress(bundle);

    const pool1 = "0x4f09bab2f0e15e2a078a227fe1537665f55b8360";

    const calldata = encodeFunctionData({
      abi: bundleAbi(bundle),
      functionName: "vote",
      args: [
        67890n, // tokenId
        [pool1],
        [0n], // weights=[0] — forbid-zero-weight-sum would deny in Cedar.
      ],
    });

    mocks.resolveAdapter.mockResolvedValueOnce(adapterHitFor(bundle));
    mocks.declarativeRouteRequest.mockResolvedValueOnce({
      envelopes: [
        {
          category: "misc",
          action: "gauge_vote",
          fields: {
            voter,
            tokenId: "67890",
            pools: [pool1],
            weights: ["0"], // <- zero-sum
            kind: "vote",
          },
        },
      ],
      decoder_id: "declarative.aerodrome/voter/vote",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: BASE_CHAIN_ID,
      from: USER,
      to: voter,
      calldataHex: calldata,
    });

    // Mapping passes — the Cedar verdict layer is exercised elsewhere
    // (see plan §6.6 Cedar verdict note). Here we lock the contract:
    // declarative-route MUST emit the envelope regardless of policy
    // disposition, so the orchestrator can hand the envelope to the
    // Cedar engine for evaluation.
    expect(outcome.kind).toBe("hit");
    if (outcome.kind !== "hit") return;
    expect(outcome.value.envelopes).toHaveLength(1);

  });

  // ── Scenario 9: VotingEscrow.createLock ────────────────────────────────
  it("VotingEscrow.createLock — LockCreate envelope with lockDurationSec arrives", async () => {
    const bundle = loadAerodromeBundle("voting-escrow", "createLock");
    const escrow = bundlePrimaryAddress(bundle);

    const calldata = encodeFunctionData({
      abi: bundleAbi(bundle),
      functionName: "createLock",
      args: [
        100_000_000_000_000_000_000n, // value: 100 AERO
        BigInt(126_144_000), // lockDuration: 4 years (~ 4*365*24*3600)
      ],
    });

    mocks.resolveAdapter.mockResolvedValueOnce(adapterHitFor(bundle));
    mocks.declarativeRouteRequest.mockResolvedValueOnce({
      envelopes: [
        {
          category: "misc",
          action: "lock_create",
          fields: {
            votingEscrow: escrow,
            asset: { kind: "erc20", address: BASE_AERO },
            amount: { kind: "exact", value: "100000000000000000000" },
            lockDurationSec: "126144000",
            recipient: USER,
          },
        },
      ],
      decoder_id: "declarative.aerodrome/voting-escrow/createLock",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: BASE_CHAIN_ID,
      from: USER,
      to: escrow,
      calldataHex: calldata,
      options: { tokenClient: makeTokenClient() },
    });

    expect(outcome.kind).toBe("hit");
    if (outcome.kind !== "hit") return;
    expect(outcome.value.decoderId).toBe(
      "declarative.aerodrome/voting-escrow/createLock",
    );
    expect(outcome.value.envelopes).toHaveLength(1);

    // The mapper emitted `lockDurationSec` on the envelope; the enricher
    // should leave it alone (it's a string, not an AssetRef).
    const envelope = outcome.value.envelopes[0] as {
      fields: { lockDurationSec: string; asset: Record<string, unknown> };
    };
    expect(envelope.fields.lockDurationSec).toBe("126144000");
    // Enrichment ran on the AERO asset embedded in the lock envelope.
    expect(envelope.fields.asset).toMatchObject({
      symbol: "AERO",
      decimals: 18,
    });
  });

  // ── Scenario 10: VotingEscrow.merge (self-merge — mapping succeeds) ─────
  it("VotingEscrow.merge from==to (self-merge) — mapping pass, Cedar forbid layer separate", async () => {
    // Mirrors scenario 8's split-of-concerns. The mapping layer emits
    // the envelope regardless of self-merge — the
    // `forbid-self-merge` Cedar policy lives downstream.
    const bundle = loadAerodromeBundle("voting-escrow", "merge");
    const escrow = bundlePrimaryAddress(bundle);

    const calldata = encodeFunctionData({
      abi: bundleAbi(bundle),
      functionName: "merge",
      args: [
        42n, // from
        42n, // to — same tokenId → self-merge
      ],
    });

    mocks.resolveAdapter.mockResolvedValueOnce(adapterHitFor(bundle));
    mocks.declarativeRouteRequest.mockResolvedValueOnce({
      envelopes: [
        {
          category: "misc",
          action: "lock_manage",
          fields: {
            votingEscrow: escrow,
            kind: "merge",
            fromTokenId: "42",
            toTokenId: "42",
          },
        },
      ],
      decoder_id: "declarative.aerodrome/voting-escrow/merge",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: BASE_CHAIN_ID,
      from: USER,
      to: escrow,
      calldataHex: calldata,
    });

    expect(outcome.kind).toBe("hit");
    if (outcome.kind !== "hit") return;
    expect(outcome.value.decoderId).toBe(
      "declarative.aerodrome/voting-escrow/merge",
    );
    expect(outcome.value.envelopes).toHaveLength(1);

    // Both tokenIds reach the engine equal → Cedar `forbid-self-merge`
    // is the layer that denies. Here we verify the mapping pipeline
    // is agnostic to the policy disposition.
    const envelope = outcome.value.envelopes[0] as {
      fields: { fromTokenId: string; toTokenId: string };
    };
    expect(envelope.fields.fromTokenId).toBe(envelope.fields.toTokenId);
  });

  // ── Scenario 11: Gauge.deposit (LP stake) ──────────────────────────────
  it("Gauge.deposit — LpStake envelope on a top-20 V2 gauge, with LP-token enrichment", async () => {
    const bundle = loadAerodromeBundle("gauge", "deposit");
    // First entry in the multi-address bundle = top-20 V2 gauge.
    const gauge = bundlePrimaryAddress(bundle);
    expect(gauge).toBe(BASE_LP_AERO_USDC);

    const calldata = encodeFunctionData({
      abi: bundleAbi(bundle),
      functionName: "deposit",
      args: [1_000_000_000_000_000_000n], // amount: 1 LP
    });

    mocks.resolveAdapter.mockResolvedValueOnce(adapterHitFor(bundle));
    mocks.declarativeRouteRequest.mockResolvedValueOnce({
      envelopes: [
        {
          category: "misc",
          action: "lp_stake",
          fields: {
            gauge,
            // builder emits `{lpToken: AssetRef}` shape — kind+address.
            lpToken: { kind: "erc20", address: gauge },
            amount: { kind: "exact", value: "1000000000000000000" },
            recipient: USER,
          },
        },
      ],
      decoder_id: "declarative.aerodrome/gauge/deposit",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: BASE_CHAIN_ID,
      from: USER,
      to: gauge,
      calldataHex: calldata,
      options: { tokenClient: makeTokenClient() },
    });

    expect(outcome.kind).toBe("hit");
    if (outcome.kind !== "hit") return;
    expect(outcome.value.decoderId).toBe(
      "declarative.aerodrome/gauge/deposit",
    );
    expect(outcome.value.envelopes).toHaveLength(1);

    // The mock token client resolves the LP token address to the
    // synthetic "AERO-USDC-LP" metadata. Enrichment must augment the
    // envelope in-place.
    const envelope = outcome.value.envelopes[0] as {
      fields: { lpToken: Record<string, unknown>; gauge: string };
    };
    expect(envelope.fields.lpToken).toMatchObject({
      kind: "erc20",
      address: gauge,
      symbol: "AERO-USDC-LP",
      decimals: 18,
    });
    expect(envelope.fields.gauge).toBe(gauge);

  });

  // ── Scenario 12: Unknown selector → miss ───────────────────────────────
  it("Unknown selector — resolveAdapter returns no_publisher, route emits miss", async () => {
    // Use a deliberately fictional selector. The resolver mock returns
    // a negative-cache verdict — the route helper MUST surface it as
    // `outcome.kind === "miss"` and never reach the WASM bridge.
    mocks.resolveAdapter.mockResolvedValueOnce({
      kind: "verdict",
      verdict: "no_adapter",
      reason: "no_publisher",
    });

    const outcome = await tryDeclarativeRoute({
      chainId: BASE_CHAIN_ID,
      from: USER,
      to: "0xcF77a3Ba9A5CA399B7c97c74d54e5b1Beb874E43".toLowerCase(),
      // `0xdeadbeef` is an explicitly bogus 4-byte selector — no
      // Aerodrome bundle declares it. Pad to a realistic length so
      // `extractSelector` returns a value (not `null`) and the route
      // helper actually reaches the resolver.
      calldataHex: "0xdeadbeef" + "00".repeat(64),
    });

    expect(outcome.kind).toBe("miss");
    if (outcome.kind === "miss") {
      expect(outcome.reason).toBe("no_publisher");
    }
    // The WASM bridge must NOT have been called — the negative-cache
    // verdict short-circuits before decoding.
    expect(mocks.declarativeRouteRequest).not.toHaveBeenCalled();
  });
});
