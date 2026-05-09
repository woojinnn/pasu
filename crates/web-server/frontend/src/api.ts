// Thin wrapper around the Rust /api/decode endpoint.
//
// The shape mirrors `DecodeResponse` in `crates/web-server/src/main.rs` —
// keep them in sync if you add fields.

export interface DecodedArg {
  name: string
  sol_type: string
  value: string
}

export type DecodeResponse =
  | {
      outcome: 'resolved'
      source: 'sourcify_curated' | 'sourcify_db' | 'openchain'
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
}

export interface ApiError {
  error: string
}

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
