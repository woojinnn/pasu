/**
 * Simulation page — integrated WASM simulator over the user's registered
 * wallets.
 *
 * Pipeline:
 *   1. Auth gate — only logged-in users see the simulator.
 *   2. Pull `listWallets()` (auth-scoped). User picks ≥1 wallet via the
 *      selector panel. Each selected wallet's state is fetched separately
 *      and threaded through the simulation.
 *   3. TX queue rows each carry a `fromWallet` discriminator — the row's
 *      `(decode → evaluate → simulate_step)` chain runs against THAT
 *      wallet's threaded state and produces THAT wallet's next state.
 *      Other wallets' states are unchanged at that step.
 *   4. `histories[addr][i] = wallet's state after step i` derives from the
 *      initial query data plus the run output, so the scrubber and the
 *      account-level aggregate both index the same axis.
 *
 * Layout:
 *   ┌──────────────────────┬──────────────────────┐
 *   │ Wallet selector      │ Account-level state  │
 *   │ (+ chain picker)     │ (aggregate)          │
 *   └──────────────────────┴──────────────────────┘
 *   ┌──────────────┬──────────────────┬──────────┐
 *   │ TX queue     │ Per-wallet state │ Verdicts │
 *   │ (per-row     │ (scrubber +      │ + policy │
 *   │  fromWallet) │  delta diff)     │  on/off  │
 *   └──────────────┴──────────────────┴──────────┘
 *
 * Base state input field is gone — wallets come from the server-backed
 * list, not an arbitrary address paste.
 */

import { useEffect, useMemo, useState } from "react";
import { useMutation, useQueries, useQuery } from "@tanstack/react-query";
import { useNavigate, useSearchParams } from "react-router-dom";

import {
  getEnabledPolicyIds,
  getWalletState,
  listManagedPolicies,
  listWallets,
  startGoogleLogin,
  type ManagedPolicy,
} from "../server-api";
import { Topbar } from "../shell/Topbar";
import { useAuth } from "../hooks/useAuth";

import {
  CalldataTxBuilder,
  MAX_TX,
  blankCalldataRow,
  type CalldataTxRow,
} from "./simulation/CalldataTxBuilder";
import { WalletSelectorPanel } from "./simulation/WalletSelectorPanel";
import { AccountStatePanel } from "./simulation/AccountStatePanel";
import {
  WalletsStatePanel,
  type SimStepDelta,
} from "./simulation/WalletsStatePanel";
import { VerdictPanel } from "./simulation/VerdictPanel";
import { PolicyTogglePanel } from "./simulation/PolicyTogglePanel";
import {
  decodeCalldataLocal,
  evaluateActionLocal,
  getV3BundleStatus,
  simulateStepLocal,
  type EvaluateActionVerdict,
  type OpaqueAction,
  type OpaqueStateDelta,
  type OpaqueWalletState,
} from "./simulation/sim-bridge";
import { SAMPLE_ERC20_TRANSFER_PROBE } from "./simulation/wasm-probe-fixture";

import "./simulation.css";

/** Mirror of the SW's `managedToV2Bundle` synth: honour an explicit
 *  manifest when present, fall back to a minimal one. */
function managedToBundle(p: ManagedPolicy): { policy: string; manifest: unknown } {
  const manifest =
    p.manifest && typeof p.manifest === "object"
      ? p.manifest
      : { id: p.id, schema_version: 2 };
  return { policy: p.text, manifest };
}

interface StepOutput {
  rowId: string;
  /** Lowercase wallet addr — the wallet that ran this step. */
  fromWallet: string;
  verdict: EvaluateActionVerdict | null;
  delta: OpaqueStateDelta;
  /** Wallet state AFTER this step. Equal to `preState` when an engine
   *  error prevented the step from running. */
  postState: OpaqueWalletState;
  /** Per-row error surfaced from the WASM engine (e.g. `token not found`,
   *  `balance underflow`). When present, the row's TX card renders a
   *  banner instead of a verdict pill. `null` on success. */
  error: string | null;
}

export function SimulationPage() {
  const auth = useAuth();

  // Login gate: bail early when we're sure there is no user. We don't
  // bail during the initial `/auth/me` round-trip — that would briefly
  // flash the login prompt for already-signed-in users.
  if (!auth.isLoading && !auth.user) {
    return <LoginGate onLogin={() => auth.login()} />;
  }

  return <SimulationPageInner />;
}

function SimulationPageInner() {
  // ── wallets ────────────────────────────────────────────────────────────
  const walletsQ = useQuery({
    queryKey: ["wallets"],
    queryFn: listWallets,
  });
  const wallets = walletsQ.data ?? [];

  // Selected wallets — addresses kept lowercased. Auto-select the first
  // wallet once the list arrives so the page doesn't open empty.
  const [selected, setSelected] = useState<Set<string>>(new Set());
  useEffect(() => {
    if (selected.size === 0 && wallets.length > 0) {
      setSelected(new Set([wallets[0].address.toLowerCase()]));
    }
    // Intentionally exclude `selected` from deps to avoid resetting after
    // the user clears everything. We only auto-select on the FIRST landing.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wallets.length]);
  const toggleWallet = (addr: string) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(addr)) next.delete(addr);
      else next.add(addr);
      return next;
    });
  const selectAll = () =>
    setSelected(new Set(wallets.map((w) => w.address.toLowerCase())));
  const clearAll = () => setSelected(new Set());

  // ── chain ──────────────────────────────────────────────────────────────
  const [chain, setChain] = useState("eip155:1");

  // ── wallet states (one query per REGISTERED wallet — not just selected) ──
  //
  // The account-level rollup is independent of the selection UI: it always
  // sums every registered wallet, so we have to keep their states cached
  // regardless of which ones are currently checked. Selection only affects
  // the per-wallet panel and the TX builder's "from" dropdown.
  const allWalletAddrs = useMemo(
    () => wallets.map((w) => w.address.toLowerCase()),
    [wallets],
  );
  const selectedArr = useMemo(() => [...selected], [selected]);
  const stateQueries = useQueries({
    queries: allWalletAddrs.map((addr) => ({
      queryKey: ["wallet-state", addr],
      queryFn: () =>
        getWalletState(addr).then((s) => s as unknown as OpaqueWalletState),
    })),
  });
  const initialStates = useMemo(() => {
    const m = new Map<string, OpaqueWalletState>();
    allWalletAddrs.forEach((addr, i) => {
      const data = stateQueries[i]?.data;
      if (data) m.set(addr, data);
    });
    return m;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- allWalletAddrs+queries
  }, [allWalletAddrs, stateQueries.map((q) => q.dataUpdatedAt).join("|")]);

  // ── policies ────────────────────────────────────────────────────────────
  const managedQ = useQuery({
    queryKey: ["managed-policies"],
    queryFn: listManagedPolicies,
  });
  const liveEnabledQ = useQuery({
    queryKey: ["enabled-policy-ids"],
    queryFn: getEnabledPolicyIds,
  });
  const v3Q = useQuery({
    queryKey: ["sim-v3-bundle-count"],
    queryFn: getV3BundleStatus,
    refetchInterval: (q) => (q.state.data?.bootCompleted ? false : 1500),
  });
  const policies: ReadonlyArray<ManagedPolicy> = managedQ.data ?? [];

  // Cedar `@id` → policy text, so the verdict panel can resolve a matched
  // deny back to its source for the structure diagram + diagnosis.
  const policyTextById = useMemo(() => {
    const m: Record<string, string> = {};
    for (const p of policies) {
      const id = p.text.match(/@id\("([^"]+)"\)/)?.[1];
      if (id) m[id] = p.text;
    }
    return m;
  }, [policies]);

  const [enabledIds, setEnabledIds] = useState<Set<string>>(new Set());
  // Seed once when both queries land.
  useEffect(() => {
    if (
      enabledIds.size === 0 &&
      policies.length > 0 &&
      (liveEnabledQ.data ?? []).length > 0
    ) {
      setEnabledIds(new Set(liveEnabledQ.data));
    }
    // Intentionally only seed once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [policies.length, liveEnabledQ.data?.length]);
  const togglePolicy = (id: string) =>
    setEnabledIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const enableAll = () => setEnabledIds(new Set(policies.map((p) => p.id)));
  const disableAll = () => setEnabledIds(new Set());

  // ── TX queue ───────────────────────────────────────────────────────────
  const [rows, setRows] = useState<CalldataTxRow[]>(() => [
    blankCalldataRow(0),
  ]);
  const safeSetRows = (next: CalldataTxRow[]) => setRows(next.slice(0, MAX_TX));
  const [selectedRowId, setSelectedRowId] = useState<string | null>(
    rows[0]?.id ?? null,
  );

  // ── URL query-param hydration (HistoryPage "다시 시뮬" entry point) ────
  //
  // History row → `<Link to="/simulation?from=…&to=…&calldata=…&value=…&chain=…">`.
  // When we mount with those params present, replace the (still-blank) first
  // row with the encoded tx, set the chain selector to match, auto-select the
  // matching registered wallet when there is one, and STRIP the params from
  // the URL so a casual refresh doesn't re-trigger the hydration.
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  void navigate; // imported for future "지금 시뮬" CTA wiring; harmless otherwise.
  const hydratedFromUrlRef = useMemo(() => ({ current: false }), []);
  useEffect(() => {
    if (hydratedFromUrlRef.current) return;
    const urlFrom = searchParams.get("from") ?? "";
    const urlTo = searchParams.get("to") ?? "";
    const urlCalldata = searchParams.get("calldata") ?? "";
    const urlValue = searchParams.get("value") ?? "";
    const urlChain = searchParams.get("chain") ?? "";
    if (!urlFrom && !urlTo && !urlCalldata) return; // no hydration intent
    hydratedFromUrlRef.current = true;
    // Wait for wallets to land before deciding whether the URL `from` is
    // already registered (and therefore selectable in the chain-shared
    // wallet picker). If it isn't, the user can still run the row — the
    // CalldataTxBuilder's from input accepts arbitrary addresses.
    if (urlChain) setChain(urlChain);
    setRows([
      {
        id: rows[0]?.id ?? blankCalldataRow(0).id,
        label: "history → 다시 시뮬",
        fromWallet: urlFrom.toLowerCase(),
        to: urlTo,
        calldata: urlCalldata || "0x",
        value: urlValue || "0",
      },
    ]);
    if (urlFrom) {
      setSelected((prev) => {
        const next = new Set(prev);
        next.add(urlFrom.toLowerCase());
        return next;
      });
    }
    // Strip the params so a refresh doesn't keep re-hydrating an
    // already-edited row. `setSearchParams({})` rewrites the location
    // without bouncing the user.
    setSearchParams({}, { replace: true });
  }, [searchParams, setSearchParams, rows, hydratedFromUrlRef]);

  // ── run ────────────────────────────────────────────────────────────────
  const [stepsOut, setStepsOut] = useState<StepOutput[]>([]);
  const [cursorIdx, setCursorIdx] = useState(0);

  // Histories derived from initialStates + stepsOut. Every selected wallet
  // gets the SAME length (= 1 + stepsOut.length) so the cursor indexes the
  // same axis across wallets / aggregate / verdict panel.
  const histories = useMemo(() => {
    const m = new Map<string, OpaqueWalletState[]>();
    for (const [addr, init] of initialStates) {
      m.set(addr, [init]);
    }
    for (const step of stepsOut) {
      for (const addr of m.keys()) {
        const hist = m.get(addr)!;
        if (addr === step.fromWallet) {
          hist.push(step.postState);
        } else {
          hist.push(hist[hist.length - 1]);
        }
      }
    }
    return m;
  }, [initialStates, stepsOut]);

  // Per-step delta + owner, used by the WalletsStatePanel scrubber.
  const stepDeltas: SimStepDelta[] = useMemo(
    () =>
      stepsOut.map((s) => ({ walletAddr: s.fromWallet, delta: s.delta })),
    [stepsOut],
  );

  // Reset run output when the inputs change in a way that invalidates it.
  const inputsSig = JSON.stringify({
    rows: rows.map((r) => ({
      from: r.fromWallet,
      to: r.to,
      data: r.calldata,
      v: r.value,
    })),
    selected: [...selected].sort(),
    enabled: [...enabledIds].sort(),
    chain,
  });
  useEffect(() => {
    if (stepsOut.length > 0) {
      setStepsOut([]);
      setCursorIdx(0);
    }
    // We INTENTIONALLY don't depend on stepsOut here — the guard inside
    // already short-circuits when it's already empty, and including it
    // would invert the effect's intent.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inputsSig]);

  const runMut = useMutation({
    mutationFn: async () => {
      const enabled = policies.filter((p) => enabledIds.has(p.id));
      const bundles = enabled.map(managedToBundle);
      const submittedAt = Math.floor(Date.now() / 1000);
      const caipChain = chain;
      const chainIdNum = caipToChainId(chain);

      // Threaded pre-states per wallet. Each row reads from this and
      // writes back at the end. Wallets that don't run any step retain
      // their initial state — that's fine; the histories memo handles the
      // axis alignment.
      const walletCur = new Map<string, OpaqueWalletState>(initialStates);

      const out: StepOutput[] = [];
      let nonce = 0;

      for (const row of rows) {
        const fromAddr = row.fromWallet.toLowerCase();
        if (!fromAddr) {
          throw new Error("from 지갑 주소가 비어있습니다");
        }
        // Non-registered addresses are allowed: the engine just needs a
        // string for the principal entity, and an empty wallet state lets
        // sim-step run without crashing. The state panel won't render
        // ad-hoc addresses (they don't appear in `histories`), but the
        // verdict path is fully functional.
        let preState = walletCur.get(fromAddr);
        if (!preState) {
          preState = emptyWalletState(fromAddr, caipChain);
          walletCur.set(fromAddr, preState);
        }

        let rowVerdict: EvaluateActionVerdict | null = null;
        let lastDelta: OpaqueStateDelta = {};
        let lastNext: OpaqueWalletState = preState;
        let rowError: string | null = null;

        // Per-row try/catch: an engine error (token not found, balance
        // underflow, decode failure, …) becomes a row-scoped banner
        // instead of poisoning the rest of the batch. State threading
        // sticks at `preState` for that row so subsequent rows on the
        // same wallet see the last-good snapshot.
        try {
          const decoded = await decodeCalldataLocal({
            chain_id: chainIdNum,
            to: row.to.trim(),
            selector: selectorOf(row.calldata),
            calldata: row.calldata.trim(),
            value: row.value.trim() || "0",
            submitter: fromAddr,
            submitted_at: submittedAt + out.length,
            nonce: nonce++,
          });

          for (const action of decoded.actions as OpaqueAction[]) {
            const meta =
              (action as { meta?: Record<string, unknown> }).meta ?? {};
            const body =
              (action as { body?: Record<string, unknown> }).body ?? action;

            let stepVerdict: EvaluateActionVerdict | null = null;
            if (bundles.length > 0) {
              stepVerdict = await evaluateActionLocal({
                action: body,
                meta,
                tx: { chain_id: caipChain, from: fromAddr, to: row.to.trim() },
                bundles,
                results: {},
              });
            }
            rowVerdict = worsen(rowVerdict, stepVerdict);

            const stepOut = await simulateStepLocal({
              state: lastNext,
              action,
              ctx: SAMPLE_ERC20_TRANSFER_PROBE.ctx,
            });
            lastNext = stepOut.next_state;
            lastDelta = stepOut.delta;
          }
        } catch (err) {
          const raw = err instanceof Error ? err.message : String(err);
          rowError = explainEngineError(raw);
        }

        walletCur.set(fromAddr, lastNext);
        out.push({
          rowId: row.id,
          fromWallet: fromAddr,
          verdict: rowVerdict,
          delta: lastDelta,
          postState: lastNext,
          error: rowError,
        });
      }
      return out;
    },
    onSuccess: (out) => {
      setStepsOut(out);
      const firstBad = out.findIndex(
        (o) => o.verdict && o.verdict.kind !== "pass",
      );
      setCursorIdx(firstBad === -1 ? out.length : firstBad + 1);
    },
  });

  // ── derived ─────────────────────────────────────────────────────────────
  const verdictByRowId = useMemo(() => {
    const m = new Map<string, EvaluateActionVerdict>();
    for (const o of stepsOut) {
      if (o.verdict) m.set(o.rowId, o.verdict);
    }
    return m;
  }, [stepsOut]);

  const errorByRowId = useMemo(() => {
    const m = new Map<string, string>();
    for (const o of stepsOut) {
      if (o.error) m.set(o.rowId, o.error);
    }
    return m;
  }, [stepsOut]);

  const currentVerdict =
    cursorIdx === 0 ? undefined : stepsOut[cursorIdx - 1]?.verdict ?? undefined;

  const overallVerdict: "pass" | "warn" | "fail" | null = useMemo(() => {
    if (stepsOut.length === 0) return null;
    let kind: "pass" | "warn" | "fail" = "pass";
    for (const o of stepsOut) {
      if (!o.verdict) continue;
      if (o.verdict.kind === "fail") {
        kind = "fail";
        break;
      }
      if (o.verdict.kind === "warn") kind = "warn";
    }
    return kind;
  }, [stepsOut]);

  const [changedOnly, setChangedOnly] = useState(false);

  const anyStateLoading = stateQueries.some((q) => q.isLoading);

  return (
    <>
      <Topbar
        here="Simulation"
        subtitle={`${rows.length} / ${MAX_TX} TX`}
      />

      <div className="sim-runstrip">
        <button
          className="btn primary"
          onClick={() => runMut.mutate()}
          disabled={
            runMut.isPending ||
            rows.length === 0 ||
            anyStateLoading ||
            // Each row's `from` must be set — registered wallet OR any
            // typed 0x address. Empty string is the only blocker.
            rows.some((r) => !r.fromWallet.trim())
          }
        >
          {runMut.isPending
            ? "실행 중…"
            : `시뮬레이션 실행 (${rows.length})`}
        </button>
        <div className="rs-meta">
          정책: <strong>{enabledIds.size}</strong> 활성
          <span className="sep">·</span>
          TX: <strong>{rows.length}</strong>
          <span className="sep">·</span>
          지갑: <strong>{selected.size}</strong>
          {v3Q.data && v3Q.data.bootCompleted && v3Q.data.count === 0 && (
            <>
              <span className="sep">·</span>
              <span className="rs-warn">
                v3 bundles 0개 — 디코드 결과 Unknown만 나옴
              </span>
            </>
          )}
          {v3Q.data && v3Q.data.bootCompleted && v3Q.data.count > 0 && (
            <>
              <span className="sep">·</span>
              v3 bundles: <strong>{v3Q.data.count}</strong>
            </>
          )}
        </div>
        {overallVerdict && (
          <div className={`rs-overall ${overallVerdict}`}>
            <span className="ov-pill">{overallVerdict.toUpperCase()}</span>
            <span className="ov-sub">
              {stepsOut.filter((s) => s.verdict?.kind === "pass").length} pass ·{" "}
              {stepsOut.filter((s) => s.verdict?.kind === "warn").length} warn ·{" "}
              {stepsOut.filter((s) => s.verdict?.kind === "fail").length} fail
            </span>
          </div>
        )}
        {runMut.error && (
          <div className="rs-err">{String(runMut.error)}</div>
        )}
      </div>

      {/* 2×3 grid:
              row 1: WalletSelector | AccountState | Verdict
              row 2: TxBuilder       | WalletsState | PolicyToggle
          The right column's two rows share a single grid track so the
          verdict box is shorter and the policy toggles take the rest. */}
      <div className="sim-grid">
        <WalletSelectorPanel
          wallets={wallets}
          selected={selected}
          toggle={toggleWallet}
          selectAll={selectAll}
          clearAll={clearAll}
          chain={chain}
          setChain={setChain}
          isRunning={runMut.isPending}
        />
        <AccountStatePanel
          histories={histories}
          cursorIdx={cursorIdx}
          setCursorIdx={setCursorIdx}
          totalSteps={stepsOut.length}
          chain={chain}
        />
        <VerdictPanel
          currentVerdict={currentVerdict}
          policyTextById={policyTextById}
        />

        <CalldataTxBuilder
          rows={rows}
          setRows={safeSetRows}
          verdictByRowId={verdictByRowId}
          errorByRowId={errorByRowId}
          selectedId={selectedRowId}
          onSelect={setSelectedRowId}
          isRunning={runMut.isPending}
          availableWallets={selectedArr}
        />

        <WalletsStatePanel
          selected={selectedArr}
          histories={histories}
          deltas={stepDeltas}
          cursorIdx={cursorIdx}
          setCursorIdx={setCursorIdx}
          changedOnly={changedOnly}
          setChangedOnly={setChangedOnly}
          chain={chain}
        />

        <PolicyTogglePanel
          policies={policies}
          enabledIds={enabledIds}
          toggle={togglePolicy}
          enableAll={enableAll}
          disableAll={disableAll}
          currentVerdict={currentVerdict}
          flashPolicyId={null}
        />
      </div>
    </>
  );
}

// ── helpers ────────────────────────────────────────────────────────────────

function LoginGate({ onLogin }: { onLogin: () => void }) {
  return (
    <div className="sim-login-gate">
      <h2>로그인이 필요합니다</h2>
      <p>
        시뮬레이터는 등록된 지갑들의 실제 state를 기반으로 동작합니다.
        Google 계정으로 로그인하면 지갑 목록이 자동으로 불러와집니다.
      </p>
      <button className="btn primary" onClick={onLogin}>
        Google로 로그인
      </button>
    </div>
  );
}

function selectorOf(calldata: string): string {
  const cd = calldata.trim();
  if (cd.startsWith("0x") && cd.length >= 10) {
    return cd.slice(0, 10).toLowerCase();
  }
  return "0x00000000";
}

/** CAIP-2 string → decimal chain id. The route decoder takes a numeric
 *  `chain_id` (legacy v3 wire shape); we synthesise it from the active
 *  CAIP-2 selection so the page-level chain selector is the single
 *  source of truth. */
function caipToChainId(caip: string): number {
  const m = caip.match(/^eip155:(\d+)$/);
  if (!m) return 1;
  return Number(m[1]);
}

/** Minimal `WalletState` shape for the WASM reducer — used when the user
 *  types an unregistered address into the `from` field. The state has no
 *  tokens / positions / approvals; the simulator still runs (most simple
 *  actions like ERC20 approve don't need pre-existing balances), and the
 *  verdict path is fully exercised against the synthesized principal
 *  entity the engine builds from `tx.from`. */
function emptyWalletState(
  addr: string,
  chain: string,
): OpaqueWalletState {
  return {
    wallet_id: { address: addr, chains: [chain] },
    tokens: [],
    approvals: { erc20: [], set_for_all: [], permit2: [] },
    positions: [],
    pending: [],
    block_heights: {},
  };
}

/** Lowercase contract address → display symbol for well-known mainnet
 *  tokens. Used when surfacing `token not found` errors so the user sees
 *  "USDT" instead of a raw `0xdac17f9…` hex. Extend as needed; an unknown
 *  address simply renders the short hex form. */
const KNOWN_TOKEN_LABELS: Record<string, string> = {
  "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48": "USDC",
  "0xdac17f958d2ee523a2206206994597c13d831ec7": "USDT",
  "0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2": "WETH",
  "0x6b175474e89094c44da98b954eedeac495271d0f": "DAI",
  "0x7fc66500c84a76ad7e9c93437bfc5ac33e2ddae9": "AAVE",
};

function shortAddrInline(addr: string): string {
  if (!addr || addr.length < 10) return addr;
  return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
}

function tokenLabel(addr: string): string {
  const lower = addr.toLowerCase();
  return KNOWN_TOKEN_LABELS[lower] ?? shortAddrInline(addr);
}

/** Translate raw engine error messages into actionable hints.
 *
 * The WASM engine surfaces low-level errors verbatim (e.g.
 * `apply_failed: token not found: Erc20 { chain: ChainId("eip155:1"),
 * address: 0xdac17f9… }`). They're precise but the user has to mentally
 * map "address 0xdac17f9…" to "USDT" and "token not found" to "this
 * wallet doesn't track USDT". This helper does both mappings, falling
 * through to the raw message when no pattern matches. */
function explainEngineError(raw: string): string {
  // `token not found: Erc20 { ..., address: 0x... }` — wallet's token list
  // doesn't include the token the action is trying to debit/credit.
  const notFound = raw.match(/token not found:.*address:\s*(0x[a-fA-F0-9]+)/);
  if (notFound) {
    const label = tokenLabel(notFound[1]);
    return `이 지갑은 ${label}을(를) 추적하지 않습니다. 지갑 페이지에서 ${label}을(를) 추가하거나, 등록된 다른 토큰의 TX로 시뮬해주세요.`;
  }
  // `balance underflow ... address: 0x... ... debit X` — token exists but
  // the wallet doesn't have enough to spend.
  const underflow = raw.match(
    /balance underflow.*address:\s*(0x[a-fA-F0-9]+).*debit\s+(\d+)/,
  );
  if (underflow) {
    const label = tokenLabel(underflow[1]);
    return `${label} 잔액 부족 — ${underflow[2]} 만큼 차감하려는데 가용 잔액이 적습니다.`;
  }
  return raw;
}

/** Worst-case verdict aggregator: `fail` > `warn` > `pass` > `null`. */
function worsen(
  prev: EvaluateActionVerdict | null,
  next: EvaluateActionVerdict | null,
): EvaluateActionVerdict | null {
  if (!next) return prev;
  if (!prev) return next;
  const order = { pass: 0, warn: 1, fail: 2 } as const;
  return order[next.kind] > order[prev.kind] ? next : prev;
}

// Suppress an unused-import lint if `startGoogleLogin` reads as unused —
// it's reached via `auth.login()` in the gate's button handler indirectly
// (useAuth proxies to it), but the explicit type import keeps the auth
// surface visible to readers.
void startGoogleLogin;
