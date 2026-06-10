/**
 * The /simulate wizard controller — owns ALL shared state + actions across the
 * 4 steps, so navigating back/forward keeps data. Today it is backed by the
 * MockProvider (fixtures in {@link mock-data}); a RealProvider (server +
 * sim-bridge WASM) later replaces the data sources without touching the step UI.
 */

import { useCallback, useMemo, useState } from "react";

import {
  MOCK_ENABLED_IDS,
  MOCK_PACKAGES,
  MOCK_POLICIES,
  MOCK_RUN,
  MOCK_STATES,
  MOCK_TX_ROWS,
  MOCK_WALLETS,
} from "./mock-data";
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

  // ── step 2: policies ──
  policies: PolicyView[];
  packages: PackageView[];
  enabled: Set<string>;
  togglePolicy: (id: string) => void;
  togglePackage: (id: string) => void;
  packageState: (id: string) => PkgState;
  /** Policies whose `walletAddress` is one of the selected wallets. */
  walletRelatedPolicies: PolicyView[];
  /** Token symbols any enabled policy references (∅ = no token filter). */
  relevantTokens: Set<string>;
  isTokenRelevant: (symbol: string) => boolean;

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

export function useSimController(): SimController {
  const [step, setStep] = useState<WizardStep>(1);
  const [selected, setSelected] = useState<Set<string>>(() => new Set([MOCK_WALLETS[0].address]));
  const [chain, setChain] = useState("eip155:1");
  const [enabled, setEnabled] = useState<Set<string>>(() => new Set(MOCK_ENABLED_IDS));
  const [txRows, setTxRowsState] = useState<TxRow[]>(MOCK_TX_ROWS);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<RunResult | null>(null);
  const [cursorIdx, setCursorIdx] = useState(0);

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
    () => MOCK_WALLETS.filter((w) => selected.has(w.address)).map((w) => MOCK_STATES[w.address]).filter(Boolean),
    [selected],
  );

  // ── step 2 ──
  const togglePolicy = useCallback((id: string) => {
    setEnabled((prev) => {
      const n = new Set(prev);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });
  }, []);
  const packageState = useCallback(
    (id: string): PkgState => {
      const pkg = MOCK_PACKAGES.find((p) => p.id === id);
      if (!pkg || pkg.policyIds.length === 0) return "off";
      const on = pkg.policyIds.filter((pid) => enabled.has(pid)).length;
      return on === 0 ? "off" : on === pkg.policyIds.length ? "on" : "partial";
    },
    [enabled],
  );
  const togglePackage = useCallback(
    (id: string) => {
      const pkg = MOCK_PACKAGES.find((p) => p.id === id);
      if (!pkg) return;
      const turnOn = packageState(id) !== "on"; // off/partial → on, on → off
      setEnabled((prev) => {
        const n = new Set(prev);
        for (const pid of pkg.policyIds) {
          if (turnOn) n.add(pid);
          else n.delete(pid);
        }
        return n;
      });
    },
    [packageState],
  );
  const walletRelatedPolicies = useMemo(
    () => MOCK_POLICIES.filter((p) => p.walletAddress && selected.has(p.walletAddress)),
    [selected],
  );
  const relevantTokens = useMemo(() => {
    const s = new Set<string>();
    for (const p of MOCK_POLICIES) if (enabled.has(p.id)) for (const t of p.tokens) s.add(t);
    return s;
  }, [enabled]);
  const isTokenRelevant = useCallback(
    (symbol: string) => relevantTokens.size === 0 || relevantTokens.has(symbol),
    [relevantTokens],
  );

  // ── step 3 ──
  const setTxRows = useCallback((rows: TxRow[]) => setTxRowsState(rows), []);
  const addRow = useCallback(() => {
    setTxRowsState((rows) => [
      ...rows,
      {
        id: `tx-${rows.length + 1}-${rows.length}`,
        label: `트랜잭션 ${rows.length + 1}`,
        fromWallet: [...selected][0] ?? "",
        to: "",
        calldata: "",
        value: "0",
      },
    ]);
  }, [selected]);
  const removeRow = useCallback((id: string) => setTxRowsState((rows) => rows.filter((r) => r.id !== id)), []);
  const updateRow = useCallback(
    (id: string, patch: Partial<TxRow>) =>
      setTxRowsState((rows) => rows.map((r) => (r.id === id ? { ...r, ...patch } : r))),
    [],
  );

  // ── step 4 (mock run) ──
  const run = useCallback(() => {
    setRunning(true);
    setResult(null);
    // Simulate a short async run; the RealProvider will await sim-bridge here.
    setTimeout(() => {
      setResult(MOCK_RUN);
      // Land the cursor on the first failing step (like the real page does).
      const firstBad = MOCK_RUN.steps.findIndex((s) => s.verdict !== "pass");
      setCursorIdx(firstBad >= 0 ? firstBad + 1 : MOCK_RUN.steps.length);
      setRunning(false);
    }, 450);
  }, []);
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
    wallets: MOCK_WALLETS, selected, toggleWallet, chain, setChain, selectedStates,
    policies: MOCK_POLICIES, packages: MOCK_PACKAGES, enabled, togglePolicy, togglePackage, packageState,
    walletRelatedPolicies, relevantTokens, isTokenRelevant,
    txRows, setTxRows, addRow, removeRow, updateRow,
    run, running, result, cursorIdx, setCursorIdx, cumulativeDenies,
  };
}
