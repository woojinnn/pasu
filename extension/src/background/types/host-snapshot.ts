// Mirrors the HostSnapshotDto consumed by `evaluate_envelope_json` in the
// WASM bridge (crates/policy_engine_wasm/src/dto.rs). Wire-compatible JSON.

export interface OracleEntry {
  /** `${chainId}:${address.toLowerCase()}` — matches engine `Token::key()`. */
  token_key: string;
  /** TS-side convenience field requested by Plan 4 fact fetchers. */
  usd_price: number;
  /** WASM bridge DTO field consumed by `evaluate_envelope_json`. */
  usd_per_unit: string;
  /** Unix seconds, matching HostSnapshotDto. */
  as_of_ts: number;
  stale_sec: number;
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
