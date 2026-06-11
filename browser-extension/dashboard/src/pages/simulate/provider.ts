/**
 * The /simulate wizard's data seam. The controller ({@link useSimController})
 * reads ALL its source data through a {@link SimProvider} so the views never
 * touch the backend directly. The live implementation is {@link realProvider}
 * (server + ps2 store + sim-bridge WASM).
 *
 * `initial()` is a SYNCHRONOUS seed for the first render (empty shells for the
 * real provider); `load()` is the async fetch; `run()` performs the simulation.
 */

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
  /** Synchronous seed for the first render (empty shells; `load()` fills them). */
  initial(): SimData;
  /** Async refresh — fetches wallets/state/policies from the backend. */
  load(): Promise<SimData>;
  /** Run the simulation: tx queue + enabled policies → per-step verdicts + diffs. */
  run(input: RunInput): Promise<RunResult>;
}
