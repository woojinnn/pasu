// Mirrors the HostSnapshotDto consumed by `evaluate_json` in the WASM
// bridge (crates/policy_engine_wasm/src/dto.rs). Wire-compatible JSON.

export interface OracleEntry {
  /** `${chainId}:${address.toLowerCase()}` — matches engine `Token::key()`. */
  token_key: string;
  usd_per_unit: string;
  as_of_ts: number;
  stale_sec?: number;
  sources?: string[];
}

export interface BalanceEntry {
  owner: string;
  token_key: string;
  /** Raw uint256 as decimal string. */
  balance: string;
}

export interface AllowanceEntry {
  owner: string;
  token_key: string;
  spender: string;
  allowance: string;
}

export interface WindowEntry {
  actor: string;
  /** Canonical wire string: e.g. "swapVolumeUsd24h". */
  name: string;
  value: string;
}

export interface HostSnapshot {
  oracle: OracleEntry[];
  balances: BalanceEntry[];
  allowances: AllowanceEntry[];
  now_ts: number;
  windows: WindowEntry[];
}
