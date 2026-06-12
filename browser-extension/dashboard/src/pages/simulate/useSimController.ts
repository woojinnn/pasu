/**
 * The /simulation wizard controller — owns ALL shared state + actions across
 * the 4 steps, so navigating back/forward keeps data. All source data is read
 * through the injected {@link SimProvider} (the live `realProvider`), so the
 * step UI never touches the backend directly.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import type { SimData, SimProvider } from "./provider";
import type {
  DenyView,
  PackageView,
  PolicyView,
  RunResult,
  TxRow,
  WalletStateView,
  WalletView,
  WizardStep,
} from "./types";

/** `Record<addr, string[]>` → `Record<addr, Set<string>>` (enabled-ids seed). */
function toSetMap(m: Record<string, string[]>): Record<string, Set<string>> {
  const out: Record<string, Set<string>> = {};
  for (const [addr, ids] of Object.entries(m)) out[addr] = new Set(ids);
  return out;
}

/** Which state widget(s) a policy concerns, inferred from its action + name.
 *  Approval/permit → 승인; perp/leverage → 포지션; swap/transfer/token → 토큰.
 *  Unclassifiable policies default to 토큰 (the broadest surface). */
function widgetsOfPolicy(p: PolicyView): string[] {
  const hay = `${p.action} ${p.name}`.toLowerCase();
  const out = new Set<string>();
  if (/approv|permit|allowance/.test(hay)) out.add("approvals");
  if (/perp|leverage|margin|position|hyperliquid|order/.test(hay)) out.add("positions");
  if (/swap|transfer|amm|erc20|send|token|bridge|burn|recipient/.test(hay)) out.add("tokens");
  if (out.size === 0) out.add("tokens");
  return [...out];
}

/** Tri-state for a package toggle (all on / some on / all off). */
export type PkgState = "on" | "partial" | "off";

export interface SimController {
  // ── wizard nav ──
  step: WizardStep;
  goTo: (s: WizardStep) => void;
  next: () => void;
  back: () => void;
  canAdvance: boolean;

  // ── step 1: wallets + state ──
  wallets: WalletView[];
  selected: Set<string>;
  toggleWallet: (addr: string) => void;
  chain: string;
  setChain: (c: string) => void;
  /** s0 state for each selected wallet, in selection order. */
  selectedStates: WalletStateView[];

  // ── step 2: policies (managed PER WALLET) ──
  policies: PolicyView[];
  packages: PackageView[];
  /** The wallet whose policy on/off set step 2 is currently editing. Always
   *  one of the selected wallets. */
  activeWallet: string;
  setActiveWallet: (addr: string) => void;
  /** The active wallet's s0 state (drives the step-2 relevance aside). */
  activeState: WalletStateView | null;
  /** Enabled policy ids FOR THE ACTIVE WALLET. */
  enabled: Set<string>;
  /** How many policies are enabled for a given wallet (switcher chips). */
  enabledCount: (addr: string) => number;
  togglePolicy: (id: string) => void;
  togglePackage: (id: string) => void;
  packageState: (id: string) => PkgState;
  /** Policies scoped to the active wallet (`walletAddress` === active). */
  walletRelatedPolicies: PolicyView[];
  /** Token symbols any enabled policy references (∅ = no token filter). */
  relevantTokens: Set<string>;
  isTokenRelevant: (symbol: string) => boolean;
  /** Protocols any enabled policy references (∅ = no protocol filter). */
  relevantProtocols: Set<string>;
  isProtocolRelevant: (protocol: string) => boolean;
  /** State categories (widgets) the enabled policies concern, by action/domain
   *  (approval policy → approvals; swap/transfer → tokens; perp → positions). */
  relevantWidgets: Set<string>;
  isWidgetRelevant: (key: string) => boolean;
  /** True when ≥1 policy is enabled — the state view then narrows to relevance. */
  hasRelevanceFilter: boolean;

  // ── step 3: tx queue ──
  txRows: TxRow[];
  setTxRows: (rows: TxRow[]) => void;
  addRow: () => void;
  removeRow: (id: string) => void;
  updateRow: (id: string, patch: Partial<TxRow>) => void;

  // ── step 4: results ──
  run: () => void;
  running: boolean;
  result: RunResult | null;
  cursorIdx: number;
  setCursorIdx: (i: number) => void;
  /** Denies accumulated across steps 1..cursor (dedup by policy, earliest step). */
  cumulativeDenies: (cursor: number) => DenyView[];
}

export function useSimController(provider: SimProvider): SimController {
  const { t } = useTranslation("simulation");
  // Provider-sourced data: seeded synchronously from `initial()` (fixtures for
  // mock, empty shells for real) and refreshed async via `load()`.
  const init = useMemo<SimData>(() => provider.initial(), [provider]);
  const [data, setData] = useState<SimData>(init);
  useEffect(() => {
    let live = true;
    void provider.load().then((d) => {
      if (live) setData(d);
    });
    return () => {
      live = false;
    };
  }, [provider]);

  const [step, setStep] = useState<WizardStep>(1);
  const [selected, setSelected] = useState<Set<string>>(
    () => new Set(init.wallets[0] ? [init.wallets[0].address] : []),
  );
  const [chain, setChain] = useState("eip155:1");
  // Policies are managed per wallet: address → enabled policy-id set.
  const [enabledByWallet, setEnabledByWallet] = useState<Record<string, Set<string>>>(
    () => toSetMap(init.enabledByWallet),
  );
  const [activeWalletRaw, setActiveWalletRaw] = useState<string>(init.wallets[0]?.address ?? "");
  const [txRows, setTxRowsState] = useState<TxRow[]>(init.txRows);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<RunResult | null>(null);
  const [cursorIdx, setCursorIdx] = useState(0);

  // First time real data arrives (initial() was empty), seed the user-mutable
  // selection / enabled-set / tx-queue from it. A mock provider seeds via the
  // synchronous initializers above, so this is a no-op there.
  const seededRef = useRef(init.wallets.length > 0);
  useEffect(() => {
    if (seededRef.current || data.wallets.length === 0) return;
    seededRef.current = true;
    setSelected(new Set([data.wallets[0].address]));
    setActiveWalletRaw(data.wallets[0].address);
    setEnabledByWallet(toSetMap(data.enabledByWallet));
    setTxRowsState(data.txRows);
  }, [data]);

  // ── nav ──
  const goTo = useCallback((s: WizardStep) => setStep(s), []);
  const next = useCallback(() => setStep((s) => (s < 4 ? ((s + 1) as WizardStep) : s)), []);
  const back = useCallback(() => setStep((s) => (s > 1 ? ((s - 1) as WizardStep) : s)), []);
  const canAdvance = useMemo(() => {
    if (step === 1) return selected.size > 0;
    if (step === 3) return txRows.length > 0 && txRows.every((r) => r.fromWallet.trim() !== "");
    return true;
  }, [step, selected, txRows]);

  // ── step 1 ──
  const toggleWallet = useCallback((addr: string) => {
    setSelected((prev) => {
      const n = new Set(prev);
      if (n.has(addr)) n.delete(addr);
      else n.add(addr);
      return n;
    });
  }, []);
  const selectedStates = useMemo(
    () =>
      data.wallets
        .filter((w) => selected.has(w.address))
        .map((w) => data.statesByAddr[w.address])
        .filter(Boolean),
    [selected, data],
  );

  // ── step 2 (per-wallet policy on/off) ──
  // activeWallet is clamped to the current selection so it can never go stale
  // when wallets are toggled off in step 1.
  const activeWallet = useMemo(
    () => (selected.has(activeWalletRaw) ? activeWalletRaw : ([...selected][0] ?? activeWalletRaw)),
    [selected, activeWalletRaw],
  );
  const setActiveWallet = useCallback((addr: string) => setActiveWalletRaw(addr), []);

  const enabledFor = useCallback(
    (addr: string): Set<string> => enabledByWallet[addr] ?? new Set<string>(),
    [enabledByWallet],
  );
  const enabled = useMemo(() => enabledFor(activeWallet), [enabledFor, activeWallet]);
  const enabledCount = useCallback((addr: string) => enabledFor(addr).size, [enabledFor]);

  /** Apply `fn` to a fresh copy of the active wallet's enabled set. */
  const mutateActive = useCallback(
    (fn: (cur: Set<string>) => Set<string>) => {
      setEnabledByWallet((prev) => {
        const cur = new Set(prev[activeWallet] ?? []);
        return { ...prev, [activeWallet]: fn(cur) };
      });
    },
    [activeWallet],
  );

  const togglePolicy = useCallback(
    (id: string) =>
      mutateActive((n) => {
        if (n.has(id)) n.delete(id);
        else n.add(id);
        return n;
      }),
    [mutateActive],
  );
  const packageState = useCallback(
    (id: string): PkgState => {
      const pkg = data.packages.find((p) => p.id === id);
      if (!pkg || pkg.policyIds.length === 0) return "off";
      const on = pkg.policyIds.filter((pid) => enabled.has(pid)).length;
      return on === 0 ? "off" : on === pkg.policyIds.length ? "on" : "partial";
    },
    [enabled, data.packages],
  );
  const togglePackage = useCallback(
    (id: string) => {
      const pkg = data.packages.find((p) => p.id === id);
      if (!pkg) return;
      const turnOn = packageState(id) !== "on"; // off/partial → on, on → off
      mutateActive((n) => {
        for (const pid of pkg.policyIds) {
          if (turnOn) n.add(pid);
          else n.delete(pid);
        }
        return n;
      });
    },
    [packageState, mutateActive, data.packages],
  );
  const activeState = useMemo(() => data.statesByAddr[activeWallet] ?? null, [activeWallet, data.statesByAddr]);
  const walletRelatedPolicies = useMemo(
    () => data.policies.filter((p) => p.walletAddress === activeWallet),
    [activeWallet, data.policies],
  );
  const relevantTokens = useMemo(() => {
    const s = new Set<string>();
    for (const p of data.policies) if (enabled.has(p.id)) for (const t of p.tokens) s.add(t);
    return s;
  }, [enabled, data.policies]);
  const relevantProtocols = useMemo(() => {
    const s = new Set<string>();
    for (const p of data.policies) if (enabled.has(p.id)) for (const pr of p.protocols) s.add(pr);
    return s;
  }, [enabled, data.policies]);
  const isTokenRelevant = useCallback(
    (symbol: string) => relevantTokens.size === 0 || relevantTokens.has(symbol),
    [relevantTokens],
  );
  const isProtocolRelevant = useCallback(
    (protocol: string) => relevantProtocols.size === 0 || relevantProtocols.has(protocol),
    [relevantProtocols],
  );
  const relevantWidgets = useMemo(() => {
    const s = new Set<string>();
    for (const p of data.policies) if (enabled.has(p.id)) for (const w of widgetsOfPolicy(p)) s.add(w);
    return s;
  }, [enabled, data.policies]);
  const isWidgetRelevant = useCallback(
    (key: string) => relevantWidgets.size === 0 || relevantWidgets.has(key),
    [relevantWidgets],
  );
  // The state view narrows whenever ≥1 policy is enabled (widget-level), even if
  // no specific token/protocol is named.
  const hasRelevanceFilter = enabled.size > 0;

  // ── step 3 ──
  const setTxRows = useCallback((rows: TxRow[]) => setTxRowsState(rows), []);
  const addRow = useCallback(() => {
    setTxRowsState((rows) => [
      ...rows,
      {
        id: `tx-${rows.length + 1}-${rows.length}`,
        label: t("wizard.txLabel", { n: rows.length + 1 }),
        fromWallet: [...selected][0] ?? "",
        to: "",
        calldata: "",
        value: "0",
      },
    ]);
  }, [selected, t]);
  const removeRow = useCallback((id: string) => setTxRowsState((rows) => rows.filter((r) => r.id !== id)), []);
  const updateRow = useCallback(
    (id: string, patch: Partial<TxRow>) =>
      setTxRowsState((rows) => rows.map((r) => (r.id === id ? { ...r, ...patch } : r))),
    [],
  );

  // ── step 4: run via the provider (mock fixtures / real sim-bridge) ──
  const run = useCallback(() => {
    setRunning(true);
    setResult(null);
    void provider
      .run({
        selected: [...selected],
        chain,
        enabledByWallet: Object.fromEntries(
          Object.entries(enabledByWallet).map(([addr, ids]) => [addr, [...ids]]),
        ),
        txRows,
        statesByAddr: data.statesByAddr,
      })
      .then((res) => {
        setResult(res);
        // Land the cursor on the first failing step (like the real page does).
        const firstBad = res.steps.findIndex((s) => s.verdict !== "pass");
        setCursorIdx(firstBad >= 0 ? firstBad + 1 : res.steps.length);
      })
      .finally(() => setRunning(false));
  }, [provider, selected, chain, enabledByWallet, txRows, data.statesByAddr]);
  const cumulativeDenies = useCallback(
    (cursor: number): DenyView[] => {
      if (!result) return [];
      const byPolicy = new Map<string, DenyView>();
      for (const s of result.steps) {
        if (s.index > cursor) break;
        for (const d of s.denies) if (!byPolicy.has(d.policyId)) byPolicy.set(d.policyId, d);
      }
      return [...byPolicy.values()];
    },
    [result],
  );

  return {
    step, goTo, next, back, canAdvance,
    wallets: data.wallets, selected, toggleWallet, chain, setChain, selectedStates,
    policies: data.policies, packages: data.packages,
    activeWallet, setActiveWallet, activeState, enabled, enabledCount,
    togglePolicy, togglePackage, packageState,
    walletRelatedPolicies, relevantTokens, isTokenRelevant,
    relevantProtocols, isProtocolRelevant, relevantWidgets, isWidgetRelevant, hasRelevanceFilter,
    txRows, setTxRows, addRow, removeRow, updateRow,
    run, running, result, cursorIdx, setCursorIdx, cumulativeDenies,
  };
}
