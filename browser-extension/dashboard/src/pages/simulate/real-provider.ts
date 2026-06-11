/**
 * RealProvider — sources the /simulate wizard from the live backend instead of
 * fixtures:
 *
 *   Phase 1   step-1 wallets + s0 token holdings        (server-api).
 *   Phase 1b  positions + approvals on the s0 snapshot   (server-api).
 *   Phase 2   policies/packages/enabled                  (ps2 store).
 *   Phase 2b  relevance keys (tokens/protocols)          (manifest, best-effort).
 *   Phase 3   run() — decode → evaluate → step           (sim-bridge WASM).
 *
 * All backend calls go through the extension bridge; a missing bridge (dev page
 * without the content-script, or logged out) degrades gracefully rather than
 * crashing the wizard.
 */

import { blocksToText } from "../../cedar";
import type { PolicyIR } from "../../cedar/blocks/ir";
import { buildProbes, diagnoseFromResult } from "../../cedar/diagnosis";
import { runDiagnosisProbes } from "../../server-api/diagnosis";
import {
  getDashboardSummary,
  getWalletApprovalsWithRisk,
  getWalletHoldings,
  getWalletPositions,
  hlAccountOf,
  type ClassifiedApprovals,
  type Position,
} from "../../server-api";
import { getOverview, getWalletState, isEffectiveOn } from "../../server-api/policy-store";
import type { PolicyDef } from "../../server-api/policy-store";
import {
  decodeCalldataLocal,
  evaluateActionLocal,
  isMissingBridge,
  simulateStepLocal,
  type OpaqueAction,
  type OpaqueWalletState,
} from "../simulation/sim-bridge";
import { parseStateDelta, formatSignedDelta } from "../simulation/state-view";

import { TOKENS } from "./humanize";
import type { SimData, SimProvider, RunInput } from "./provider";
import type {
  ApprovalView,
  DenyView,
  PackageView,
  PolicyView,
  PositionView,
  RunResult,
  StepView,
  TokenDelta,
  TokenHolding,
  WalletStateView,
  WalletView,
} from "./types";

// ── small formatters ───────────────────────────────────────────────────────

function toHuman(rawUnits: number, decimals: number): number {
  return isFinite(rawUnits) ? rawUnits / 10 ** decimals : 0;
}
function shortAddr(a: string): string {
  return a.length > 12 ? `${a.slice(0, 6)}…${a.slice(-4)}` : a;
}
function fmtAmount(n: number): string {
  return n.toLocaleString("en-US", { maximumFractionDigits: 6 });
}
function fmtUsd(d: string | undefined): string | undefined {
  if (!d) return undefined;
  const n = Number(d);
  return isFinite(n) ? `$${n.toLocaleString("en-US", { maximumFractionDigits: 2 })}` : undefined;
}
/** CAIP-2 "eip155:1" → decimal chain id 1 (for the v3 decoder). */
function caipToDecimal(caip: string): number {
  const m = /eip155:(\d+)/.exec(caip);
  return m ? Number(m[1]) : 1;
}

// ── Phase 1 + 1b: wallet state (tokens / positions / approvals) ──────────────

function mapHolding(h: {
  symbol: string;
  decimals: number;
  balance: { amount?: string };
  value_usd?: string;
}): TokenHolding {
  const bal = toHuman(Number(h.balance.amount ?? "0"), h.decimals);
  const usdNum = h.value_usd ? Number(h.value_usd) : undefined;
  return {
    symbol: h.symbol,
    address: "",
    balance: fmtAmount(bal),
    usd: fmtUsd(h.value_usd),
    usdNum: usdNum !== undefined && isFinite(usdNum) ? usdNum : undefined,
  };
}

/** Hyperliquid perp positions → PositionView[] (Phase 1b). Non-HL positions are
 *  skipped for now (their server shape is opaque). markPrice/pnl/liq are not
 *  yet exposed, so the card renders the fields we have. */
function mapPositions(positions: Position[]): PositionView[] {
  const acct = hlAccountOf(positions);
  if (!acct) return [];
  const levOf = new Map((acct.leverage_settings ?? []).map((l) => [l.asset_index, l.leverage]));
  const margin = fmtUsd(acct.perp_account_value_usd ?? acct.perp_usdc);
  return (acct.positions ?? []).map((p, i) => {
    const size = Number(p.size);
    const entry = Number(p.entry_price);
    const lev = levOf.get(p.asset_index);
    return {
      id: `hl-${p.asset_index}-${i}`,
      label: p.symbol ?? `자산 #${p.asset_index}`,
      protocol: "hyperliquid",
      kind: "perp",
      side: p.is_long ? "long" : "short",
      leverage: lev ? `${lev}x` : undefined,
      sizeUsd: isFinite(size * entry) ? fmtUsd(String(size * entry)) : undefined,
      entryPrice: isFinite(entry) ? fmtAmount(entry) : undefined,
      marginUsd: margin,
      collateralUsd: margin,
    };
  });
}

/** A raw base-unit allowance so large it's effectively unlimited (≥ 2^256 has
 *  ~78 digits; a 30-digit cap is far past any real balance). */
function isEffectivelyUnlimited(amount?: string): boolean {
  return !!amount && /^\d+$/.test(amount) && amount.length >= 30;
}
const SYMBOL_BY_ADDR: Record<string, string> = Object.fromEntries(
  Object.entries(TOKENS).map(([sym, addr]) => [addr, sym]),
);
/** Known token symbol for an address, else a shortened hex. */
function tokenLabel(addr: string): string {
  return SYMBOL_BY_ADDR[addr.toLowerCase()] ?? shortAddr(addr);
}
/** Display string for an approval amount — collapses MAX/huge to "무제한". */
function apprAmount(amount: string | undefined, unlimited: boolean): string {
  return unlimited ? "무제한" : (amount ?? "—");
}

/** ClassifiedApprovals → ApprovalView[] (Phase 1b). */
function mapApprovals(ap: ClassifiedApprovals): ApprovalView[] {
  const out: ApprovalView[] = [];
  ap.erc20?.forEach((a, i) => {
    const unlimited = a.is_unlimited || isEffectivelyUnlimited(a.amount);
    out.push({
      id: `erc20-${i}`,
      token: tokenLabel(a.token),
      spender: "",
      spenderAddress: a.spender,
      unlimited,
      amount: apprAmount(a.amount, unlimited),
      tokenAddress: a.token.toLowerCase(),
      scope: "ERC-20",
      risk: unlimited || (a.risk?.length ?? 0) > 0 ? "high" : "low",
    });
  });
  ap.permit2?.forEach((a, i) => {
    const unlimited = isEffectivelyUnlimited(a.amount);
    out.push({
      id: `permit2-${i}`,
      token: tokenLabel(a.token),
      spender: "",
      spenderAddress: a.spender,
      unlimited,
      amount: apprAmount(a.amount, unlimited),
      tokenAddress: a.token.toLowerCase(),
      scope: "Permit2",
      risk: unlimited || (a.risk?.length ?? 0) > 0 ? "high" : "low",
    });
  });
  ap.set_for_all?.forEach((a, i) =>
    out.push({
      id: `setall-${i}`,
      token: tokenLabel(a.collection),
      spender: "",
      spenderAddress: a.operator,
      unlimited: true,
      amount: "전체 컬렉션",
      tokenAddress: a.collection.toLowerCase(),
      scope: "NFT 전체",
      risk: (a.risk?.length ?? 0) > 0 ? "high" : "med",
    }),
  );
  return out;
}

// ── Phase 2 + 2b: ps2 policies / packages / per-wallet enabled ───────────────

/** Tolerant deep search for a string field anywhere in an object tree. */
function deepFindStrings(obj: unknown, key: string, out: Set<string>, depth = 0): void {
  if (depth > 6 || obj === null || typeof obj !== "object") return;
  for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
    if (k === key && typeof v === "string") out.add(v);
    else if (typeof v === "object") deepFindStrings(v, key, out, depth + 1);
    else if (Array.isArray(v)) for (const e of v) deepFindStrings(e, key, out, depth + 1);
  }
}

function actionLabelOf(def: PolicyDef): string {
  const man = def.skeleton.manifest as Record<string, unknown> | undefined;
  const trig = man?.trigger as Record<string, unknown> | undefined;
  const action = trig?.action as Record<string, unknown> | undefined;
  const eq = action?.eq;
  if (typeof eq === "string") return eq;
  return def.cat ?? "";
}

/** Best-effort relevance keys (Phase 2b): token addresses + venue/protocol
 *  names referenced anywhere in the def's manifest. */
function relevanceOf(def: PolicyDef): { tokens: string[]; protocols: string[] } {
  const man = def.skeleton.manifest;
  const addrs = new Set<string>();
  deepFindStrings(man, "address", addrs);
  deepFindStrings(man, "token", addrs);
  const venues = new Set<string>();
  deepFindStrings(man, "venue", venues);
  deepFindStrings(man, "name", venues); // venue.name etc.
  // Map referenced token addresses → SYMBOLS so they match the dashboard's
  // token rows (which key relevance by symbol). Unknown tokens contribute no
  // relevance rather than a never-matching address.
  const tokens = [...addrs]
    .filter((s) => /^0x[0-9a-fA-F]{40}$/.test(s))
    .map((s) => SYMBOL_BY_ADDR[s.toLowerCase()])
    .filter((sym): sym is string => Boolean(sym));
  const protocols = [...venues].filter((s) => !/^0x/.test(s) && s.length < 30);
  return { tokens, protocols };
}

function buildPolicyData(snap: Awaited<ReturnType<typeof getOverview>>): {
  policies: PolicyView[];
  packages: PackageView[];
  enabledByWallet: Record<string, string[]>;
} {
  const defs = Object.values(snap.library.defs);
  const policies: PolicyView[] = defs.map((d) => {
    const rel = relevanceOf(d);
    return { id: d.id, name: d.displayName, action: actionLabelOf(d), tokens: rel.tokens, protocols: rel.protocols };
  });

  const pkgMembers = new Map<string, Set<string>>();
  const addMember = (pkgId: string, defId: string) => {
    let s = pkgMembers.get(pkgId);
    if (!s) pkgMembers.set(pkgId, (s = new Set()));
    s.add(defId);
  };
  for (const ws of Object.values(snap.wallets.byAddress))
    for (const b of Object.values(ws.bindings)) addMember(b.packageId, b.defId);
  for (const d of defs) if (d.defaults.packageId) addMember(d.defaults.packageId, d.id);

  const packages: PackageView[] = Object.values(snap.library.packages)
    .map((p) => ({ id: p.id, name: p.displayName, policyIds: [...(pkgMembers.get(p.id) ?? [])] }))
    .filter((p) => p.policyIds.length > 0);

  const enabledByWallet: Record<string, string[]> = {};
  for (const [addr, ws] of Object.entries(snap.wallets.byAddress)) {
    const ids = new Set<string>();
    for (const b of Object.values(ws.bindings)) if (isEffectiveOn(ws, b)) ids.add(b.defId);
    enabledByWallet[addr.toLowerCase()] = [...ids];
  }
  return { policies, packages, enabledByWallet };
}

// ── Phase 3: run — real evaluation via sim-bridge ────────────────────────────

interface Bundle {
  policy: string;
  manifest: unknown;
}

/** The Cedar `@id` the rendered policy carries (its verdict `policy_id`), from
 *  the def's IR annotations — falls back to the ps2 def id. */
function policyIdOf(def: PolicyDef): string {
  const ir = def.skeleton.ir as { annotations?: { name: string; value: string }[] } | undefined;
  return ir?.annotations?.find((a) => a.name === "id")?.value ?? def.id;
}

/** Phase 4: red-trace the clause(s) that blocked a tx, against the SAME context
 *  the verdict used. Mirrors PolicyDiagnosis: buildProbes → run → blame. */
async function diagnoseCulprits(
  ir: PolicyIR,
  ctx: { action: unknown; meta: unknown; tx: { chain_id: string; from: string; to: string }; bundle: Bundle },
): Promise<string[]> {
  try {
    const { probes, diagnosable } = buildProbes(ir);
    if (!diagnosable || probes.length === 0) return [];
    const result = await runDiagnosisProbes({
      action: ctx.action,
      meta: ctx.meta,
      tx: ctx.tx,
      bundles: [ctx.bundle],
      results: {},
      probes,
    });
    return diagnoseFromResult(ir, probes.map((p) => p.id), result).culprits;
  } catch {
    return [];
  }
}

const FALLBACK_IR = { effect: "forbid", scope: {}, conditions: [], annotations: [] } as unknown as PolicyIR;

/** address(lowercase) → {symbol, decimals} from an opaque engine wallet state. */
function tokenRegistry(state: OpaqueWalletState): Map<string, { symbol: string; decimals: number }> {
  const reg = new Map<string, { symbol: string; decimals: number }>();
  const tokens = (state as { tokens?: unknown }).tokens;
  if (!Array.isArray(tokens)) return reg;
  for (const entry of tokens) {
    const pair = entry as [unknown, unknown];
    const key = pair?.[0];
    const dataAndKey = entry;
    const addrSet = new Set<string>();
    deepFindStrings(key, "address", addrSet);
    const addr = [...addrSet].find((s) => /^0x[0-9a-fA-F]{40}$/.test(s))?.toLowerCase();
    if (!addr) continue;
    const symSet = new Set<string>();
    deepFindStrings(dataAndKey, "symbol", symSet);
    const symbol = [...symSet][0] ?? shortAddr(addr);
    let decimals = 18;
    const probe = (o: unknown): void => {
      if (o && typeof o === "object")
        for (const [k, v] of Object.entries(o as Record<string, unknown>)) {
          if (k === "decimals" && typeof v === "number") decimals = v;
          else if (v && typeof v === "object") probe(v);
        }
    };
    probe(dataAndKey);
    reg.set(addr, { symbol, decimals });
  }
  return reg;
}

/** OpaqueStateDelta → wizard TokenDelta[] using a token registry. */
function deltaTokens(
  delta: unknown,
  reg: Map<string, { symbol: string; decimals: number }>,
): TokenDelta[] {
  const view = parseStateDelta(delta as Record<string, unknown>);
  const out: TokenDelta[] = [];
  for (const ch of view.tokenChanges) {
    if (ch.kind !== "balance_delta") continue;
    const addr = ch.key.address?.toLowerCase?.() ?? "";
    const meta = reg.get(addr);
    const decimals = meta?.decimals ?? 18;
    const symbol = meta?.symbol ?? shortAddr(addr || "?");
    out.push({
      symbol,
      delta: formatSignedDelta(ch.delta, decimals),
      sign: ch.delta.startsWith("-") ? "down" : "up",
    });
  }
  return out;
}

const EMPTY: SimData = {
  wallets: [],
  statesByAddr: {},
  policies: [],
  packages: [],
  enabledByWallet: {},
  txRows: [],
};

export const realProvider: SimProvider = {
  initial: () => EMPTY,

  async load(): Promise<SimData> {
    const summary = await getDashboardSummary();
    const wallets: WalletView[] = summary.wallets.map((w) => ({
      address: w.address.toLowerCase(),
      name: w.label ?? shortAddr(w.address),
      chains: [],
    }));

    const statesByAddr: Record<string, WalletStateView> = {};
    await Promise.all(
      summary.wallets.map(async (w) => {
        const addr = w.address.toLowerCase();
        const [holdings, positions, approvals] = await Promise.all([
          getWalletHoldings(w.address).catch(() => []),
          getWalletPositions(w.address).catch(() => [] as Position[]),
          getWalletApprovalsWithRisk(w.address).catch(
            () => ({ erc20: [], permit2: [], set_for_all: [] }) as ClassifiedApprovals,
          ),
        ]);
        statesByAddr[addr] = {
          address: addr,
          name: w.label ?? shortAddr(w.address),
          tokens: holdings.map(mapHolding).sort((a, b) => (b.usdNum ?? 0) - (a.usdNum ?? 0)),
          positions: mapPositions(positions),
          approvals: mapApprovals(approvals),
          portfolioUsd: fmtUsd(w.total_usd),
        };
      }),
    );

    const txRows = wallets[0]
      ? [{ id: "tx-1", label: "트랜잭션 1", fromWallet: wallets[0].address, to: "", calldata: "", value: "0" }]
      : [];

    let policyData = {
      policies: [] as PolicyView[],
      packages: [] as PackageView[],
      enabledByWallet: {} as Record<string, string[]>,
    };
    try {
      policyData = buildPolicyData(await getOverview());
    } catch {
      // bridge unavailable / not logged in — keep step 2 empty.
    }

    return { wallets, statesByAddr, ...policyData, txRows };
  },

  async run(input: RunInput): Promise<RunResult> {
    const empty: RunResult = { wallets: input.selected, histories: {}, steps: [] };
    try {
      // Render each enabled def to Cedar text once (def.id → bundle).
      const { library } = await getOverview();
      const defs = library.defs;
      const defBundle = new Map<string, Bundle>();
      const bundleOfDef = async (def: PolicyDef): Promise<Bundle> => {
        let b = defBundle.get(def.id);
        if (!b) {
          const text = await blocksToText(def.skeleton.ir as PolicyIR).catch(() => "");
          defBundle.set(def.id, (b = { policy: text, manifest: def.skeleton.manifest }));
        }
        return b;
      };
      const walletBundles = new Map<string, Bundle[]>();
      const bundlesForWallet = async (addr: string): Promise<Bundle[]> => {
        let b = walletBundles.get(addr);
        if (!b) {
          const rendered = await Promise.all(
            (input.enabledByWallet[addr] ?? []).map((id) => (defs[id] ? bundleOfDef(defs[id]) : null)),
          );
          walletBundles.set(addr, (b = rendered.filter((x): x is Bundle => !!x && !!x.policy)));
        }
        return b;
      };
      // verdict `policy_id` (Cedar @id) → def, for deny resolution + diagram.
      const defByPolicyId = new Map<string, PolicyDef>();
      for (const def of Object.values(defs)) {
        defByPolicyId.set(def.id, def);
        defByPolicyId.set(policyIdOf(def), def);
      }

      // Per-wallet opaque engine state, threaded across steps.
      const cur = new Map<string, OpaqueWalletState>();
      const regs = new Map<string, ReturnType<typeof tokenRegistry>>();
      for (const addr of input.selected) {
        const s = (await getWalletState(addr).catch(() => null)) as OpaqueWalletState | null;
        const state = s ?? { wallet_id: { address: addr, chains: [input.chain] }, tokens: [], approvals: { erc20: [], set_for_all: [], permit2: [] }, positions: [], pending: [], block_heights: {} };
        cur.set(addr, state);
        regs.set(addr, tokenRegistry(state));
      }

      const baseCtx = { chain: input.chain, now: 1738000000, action_index: 0, request_kind: "transaction", simulation: "preview" };
      const chainDec = caipToDecimal(input.chain);
      const steps: StepView[] = [];
      let stepIdx = 0;

      for (const row of input.txRows) {
        const from = row.fromWallet.toLowerCase();
        if (!cur.has(from)) continue; // tx from an unselected wallet — skip.
        const decoded = await decodeCalldataLocal({
          chain_id: chainDec,
          to: row.to.trim(),
          selector: row.calldata.slice(0, 10),
          calldata: row.calldata.trim(),
          value: row.value || "0",
          submitter: from,
          submitted_at: 1738000000,
        }).catch(() => ({ actions: [], decoder_id: "" }));

        const bundles = await bundlesForWallet(from);
        const tx = { chain_id: input.chain, from, to: row.to.trim() };

        for (const action of decoded.actions) {
          stepIdx += 1;
          const body = ((action as { body?: unknown }).body ?? action) as OpaqueAction;
          const meta = (action as { meta?: unknown }).meta ?? {};
          const verdict = await evaluateActionLocal({ action: body, meta: meta as Record<string, unknown>, tx, bundles, results: {} }).catch(
            () => ({ kind: "pass" as const }),
          );
          const pre = cur.get(from)!;
          const stepped = await simulateStepLocal({ state: pre, action, ctx: { ...baseCtx, action_index: stepIdx - 1 } }).catch(
            () => ({ delta: {}, next_state: pre }),
          );
          cur.set(from, stepped.next_state);

          const matched = "matched" in verdict ? verdict.matched : [];
          const denies: DenyView[] = await Promise.all(
            matched.map(async (m): Promise<DenyView> => {
              const def = defByPolicyId.get(m.policy_id);
              const ir = def ? (def.skeleton.ir as PolicyIR) : undefined;
              // Phase 4: red-trace WHERE it blocked, against this step's context.
              const highlightPaths =
                ir && def
                  ? await diagnoseCulprits(ir, { action: body, meta, tx, bundle: await bundleOfDef(def) })
                  : [];
              return {
                policyId: m.policy_id,
                policyName: def?.displayName ?? m.policy_id,
                reason: m.reason ?? "",
                severity: m.severity === "deny" ? "deny" : "warn",
                step: stepIdx,
                ir: ir ?? FALLBACK_IR,
                highlightPaths,
              };
            }),
          );

          steps.push({
            index: stepIdx,
            rowId: row.id,
            fromWallet: from,
            label: row.label,
            verdict: verdict.kind,
            diff: { tokens: deltaTokens(stepped.delta, regs.get(from) ?? new Map()) },
            denies,
          });
        }
      }

      // Histories: s0 repeated per step (display balances stay at s0; the per-
      // step `diff.tokens` communicates the change). Length = steps+1.
      const histories: Record<string, WalletStateView[]> = {};
      for (const addr of input.selected) {
        const s0 = input.statesByAddr[addr];
        if (s0) histories[addr] = Array.from({ length: steps.length + 1 }, () => s0);
      }
      return { wallets: input.selected, histories, steps };
    } catch (err) {
      if (isMissingBridge(err)) return empty;
      throw err;
    }
  },
};
