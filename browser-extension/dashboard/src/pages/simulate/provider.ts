/**
 * The /simulate wizard's data seam. The controller ({@link useSimController})
 * reads ALL its source data through a {@link SimProvider}, so swapping fixtures
 * for the real backend (server state + ps2 store + sim-bridge WASM) never
 * touches the controller logic or the step views.
 *
 *   - {@link mockProvider}  fixtures (offline demo) — the default.
 *   - RealProvider          server + ps2 + sim-bridge (wired phase-by-phase).
 *
 * `initial()` is a SYNCHRONOUS seed (so the first render has data and the mock
 * path has zero flash); `load()` is the async refresh that the RealProvider
 * uses to fetch from the server. `run()` performs the actual simulation.
 */

import {
  MOCK_ENABLED_BY_WALLET,
  MOCK_PACKAGES,
  MOCK_POLICIES,
  MOCK_RUN,
  MOCK_STATES,
  MOCK_TX_ROWS,
  MOCK_WALLETS,
} from "./mock-data";
import type {
  PackageView,
  PolicyView,
  RunResult,
  TxRow,
  WalletStateView,
  WalletView,
} from "./types";

/** Everything the wizard needs to seed its steps, sourced from a provider. */
export interface SimData {
  wallets: WalletView[];
  /** s0 snapshot per wallet (lowercase address → state). */
  statesByAddr: Record<string, WalletStateView>;
  policies: PolicyView[];
  packages: PackageView[];
  /** Default enabled policy-ids per wallet (lowercase address → ids). */
  enabledByWallet: Record<string, string[]>;
  /** Seed tx-queue rows. */
  txRows: TxRow[];
}

/** Inputs the wizard collects across steps 1–3, handed to {@link SimProvider.run}. */
export interface RunInput {
  /** Selected wallet addresses (lowercase), in selection order. */
  selected: string[];
  /** CAIP-2 chain filter chosen in step 1. */
  chain: string;
  /** Enabled policy-ids per wallet (lowercase address → ids). */
  enabledByWallet: Record<string, string[]>;
  /** The tx queue (step 3). */
  txRows: TxRow[];
  /** s0 state per wallet (so the provider can render histories from it). */
  statesByAddr: Record<string, WalletStateView>;
}

export interface SimProvider {
  /** Synchronous seed for the first render (fixtures for mock; empty for real). */
  initial(): SimData;
  /** Async refresh — the real provider fetches wallets/state/policies here. */
  load(): Promise<SimData>;
  /** Run the simulation: tx queue + enabled policies → per-step verdicts + diffs. */
  run(input: RunInput): Promise<RunResult>;
}

const MOCK_DATA: SimData = {
  wallets: MOCK_WALLETS,
  statesByAddr: MOCK_STATES,
  policies: MOCK_POLICIES,
  packages: MOCK_PACKAGES,
  enabledByWallet: MOCK_ENABLED_BY_WALLET,
  txRows: MOCK_TX_ROWS,
};

/** Fixtures-backed provider — the default; works offline with no bridge. */
export const mockProvider: SimProvider = {
  initial: () => MOCK_DATA,
  load: () => Promise.resolve(MOCK_DATA),
  // Keep the short delay so the "실행 중…" state is visible, matching the old run.
  run: () =>
    new Promise((resolve) => {
      setTimeout(() => resolve(MOCK_RUN), 450);
    }),
};
