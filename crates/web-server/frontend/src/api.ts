// Thin wrapper around the Rust /api/decode endpoint.
//
// The shape mirrors `DecodeResponse` in `crates/web-server/src/main.rs` —
// keep them in sync if you add fields.

export interface DecodedArg {
  name: string
  sol_type: string
  value: string
}

/**
 * Schema-shaped output from the `mappers` crate (mirrors
 * `schema_demo/schema/root.json`). Populated at the top level only when a
 * mapper is registered for the (chain, target, selector) triple.
 */
export interface MappingRoot {
  schemaVersion: string
  requestKind: 'transaction' | 'signature' | 'userOperation'
  chainId: number
  from: string
  to: string
  value: string
  selector: string
  protocol?: { name: string; version?: string; component?: string }
  actions: MappingActionEnvelope[]
  blockTimestamp?: number
}

export interface MappingActionEnvelope {
  action: string
  category: string
  fields: { _kind: string; [k: string]: unknown }
}

export type DecodeResponse =
  | {
      outcome: 'resolved'
      source:
        | 'sourcify_curated'
        | 'sourcify_db'
        | 'openchain'
        | 'ur_command'
        | 'etherscan'
      function_name: string
      signature: string
      selector: string
      args: DecodedArg[]
      /**
       * Recursively decoded sub-calls. Populated when the outer call is one of
       * the recognised self-call multicall wrappers (Cat A: `multicall(bytes[])`
       * and variants). Server omits the field when empty.
       */
      children?: DecodeResponse[]
      /**
       * Schema-mapper output. Present only at the top level when a mapper
       * matches (chain, target, selector). Omitted otherwise.
       */
      mapping?: MappingRoot
    }
  | {
      outcome: 'not_found'
      selector: string
      message: string
      children?: DecodeResponse[]
    }

export interface DecodeRequest {
  chain_id: number
  address: string
  calldata: string
  /**
   * Originating wallet RPC method name (e.g. `eth_sendTransaction`).
   * The backend uses this to gate the Etherscan API fallback so it
   * only fires for write/sign operations — read calls and wallet RPCs
   * skip the fallback to keep the 5 req/s free-tier budget intact.
   */
  rpc_method?: string
}

export interface ApiError {
  error: string
}

// ── /api/sign ─────────────────────────────────────────────────────────────────

export interface SignDecodeRequest {
  method: string
  /** Raw RPC params array. */
  params: unknown
  chain_id: number
}

export type SignPayloadKind =
  | 'typed_data'
  | 'raw_message'
  | 'raw_hash'
  | 'transaction'
  | 'user_operation'
  | 'permission_request'

export interface SignDecodeResponse {
  method: string
  signer: string
  chain_id: number
  payload: { kind: SignPayloadKind } & Record<string, unknown>
}

export async function decodeSign(req: SignDecodeRequest): Promise<SignDecodeResponse> {
  const r = await fetch('/api/sign', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  })
  if (!r.ok) {
    let body: ApiError | null = null
    try {
      body = (await r.json()) as ApiError
    } catch {
      // body wasn't JSON
    }
    throw new Error(body?.error ?? `HTTP ${r.status}`)
  }
  return (await r.json()) as SignDecodeResponse
}

// ── /api/decode ───────────────────────────────────────────────────────────────

export async function decode(req: DecodeRequest): Promise<DecodeResponse> {
  const r = await fetch('/api/decode', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  })
  if (!r.ok) {
    let body: ApiError | null = null
    try {
      body = (await r.json()) as ApiError
    } catch {
      // body wasn't JSON
    }
    throw new Error(body?.error ?? `HTTP ${r.status}`)
  }
  return (await r.json()) as DecodeResponse
}
