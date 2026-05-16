import { useEffect, useRef, useState } from 'react'
import { decode, decodeSign } from './api'
import type { DecodeRequest, DecodeResponse, SignDecodeRequest, SignDecodeResponse } from './api'
import { DecodeForm } from './components/DecodeForm'
import { DecodeResult } from './components/DecodeResult'
import { SignDecodeForm } from './components/SignDecodeForm'
import { SignDecodeResult } from './components/SignDecodeResult'
import { VerdictCard, type RouteResult } from './components/VerdictCard'
import './App.css'

interface RpcEvent {
  method?: string
  origin?: string
  primaryChainId?: string
  chainIds?: string[]
  to?: string
  calldata?: string[]
  addresses?: string[]
  from?: string
  rawParams?: unknown
  parsedTypedData?: unknown
  [extra: string]: unknown
}

interface EventEntry {
  receivedAt: number
  payload: RpcEvent
  /// Result of /api/route + policy evaluation. Populated asynchronously after
  /// the SSE event lands. `undefined` while in-flight, `null` if the route
  /// call failed at the HTTP layer.
  routed?: RouteResult | null
  routedInflight?: boolean
}

/// Local-storage key the policy-builder UI uses to persist saved rules.
/// We read it here so the captured RPC events can be evaluated against the
/// user's policy library without round-tripping through the policy-builder.
const POLICY_STORAGE_KEY = 'scopeball:policy-builder:rules:v1'

interface SavedPolicy {
  id: string
  enabled: boolean
  cedarText?: string
}

function loadEnabledPolicies(): string[] {
  try {
    const raw = localStorage.getItem(POLICY_STORAGE_KEY)
    if (!raw) return []
    const list = JSON.parse(raw) as SavedPolicy[]
    return list
      .filter((p) => p.enabled && typeof p.cedarText === 'string' && p.cedarText.length > 0)
      .map((p) => p.cedarText as string)
  } catch {
    return []
  }
}

async function evaluateWriteEvent(payload: RpcEvent): Promise<RouteResult | null> {
  try {
    const body = {
      method: payload.method ?? 'eth_sendTransaction',
      params: payload.rawParams ?? [],
      chain_id: parseInt((payload.primaryChainId ?? '0x1').slice(2), 16) || 1,
      from: payload.from,
      policies: loadEnabledPolicies(),
    }
    const res = await fetch('/api/route', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    })
    if (!res.ok) {
      const text = await res.text().catch(() => '')
      return { actions: [], policy_error: `HTTP ${res.status}: ${text || 'route failed'}` }
    }
    return (await res.json()) as RouteResult
  } catch (e) {
    return { actions: [], policy_error: String(e) }
  }
}

// Two separate ring buffers so a flood of read/wallet noise can never
// push captured transactions out of the panel.
const TX_LIMIT = 50      // write + sign
const OTHER_LIMIT = 30   // everything else (read/wallet/connect)

type MethodCategory = 'write' | 'sign' | 'connect' | 'wallet' | 'read'

function categorize(method?: string): MethodCategory {
  if (!method) return 'read'
  if (method === 'eth_sendTransaction' || method === 'eth_signTransaction') return 'write'
  if (
    method === 'personal_sign' ||
    method === 'eth_sendUserOperation' ||
    method.startsWith('eth_sign')
  ) return 'sign'
  if (method === 'eth_requestAccounts' || method === 'eth_accounts') return 'connect'
  if (method.startsWith('wallet_')) return 'wallet'
  return 'read'
}

const CATEGORY_LABEL: Record<MethodCategory, string> = {
  write: 'WRITE',
  sign: 'SIGN',
  connect: 'CONNECT',
  wallet: 'WALLET',
  read: 'READ',
}

// Returns true when `next` looks like a duplicate of the most recent entry
// `prev`: same method + same `to` + same calldata, arrived within `windowMs`.
//
// Uniswap (and other dApps) frequently call eth_sendTransaction twice in a
// row — once for gas estimation, once for the real send — and userscripts
// can double-hook the provider. Either way the user sees the same payload
// twice. Folding those collapses the visual noise without losing real txs:
// a genuine second transaction would either differ in calldata or land far
// outside the window.
function isRecentDuplicate(
  prev: EventEntry | undefined,
  next: EventEntry,
  windowMs: number,
): boolean {
  if (!prev) return false
  if (next.receivedAt - prev.receivedAt > windowMs) return false
  if (prev.payload.method !== next.payload.method) return false
  if ((prev.payload.to ?? '') !== (next.payload.to ?? '')) return false
  const a = prev.payload.calldata
  const b = next.payload.calldata
  const av = Array.isArray(a) ? a.join('|') : ''
  const bv = Array.isArray(b) ? b.join('|') : ''
  return av === bv
}

function parseChainId(payload: RpcEvent): string {
  const cidRaw = payload.primaryChainId ?? payload.chainIds?.[0]
  if (typeof cidRaw === 'string') {
    const n = cidRaw.startsWith('0x') ? parseInt(cidRaw, 16) : Number(cidRaw)
    if (Number.isFinite(n) && n > 0) return String(n)
  }
  return '1'
}

// Write event: needs a target address + calldata.
function isWriteDecodable(entry: EventEntry): boolean {
  const cat = categorize(entry.payload.method)
  if (cat !== 'write') return false
  const cd = entry.payload.calldata
  return Array.isArray(cd) && typeof cd[0] === 'string' && cd[0].length > 0
}

// Sign event: needs a recognized sign method.
function isSignDecodable(entry: EventEntry): boolean {
  return categorize(entry.payload.method) === 'sign' && !!entry.payload.method
}

interface WriteFill {
  chainId: string
  address: string
  calldata: string
}

interface SignFill {
  method: string
  chainId: string
  params: string
}

function fillWriteFromEvent(payload: RpcEvent): WriteFill | null {
  const cd = Array.isArray(payload.calldata) ? payload.calldata[0] : null
  if (!cd) return null
  return {
    chainId: parseChainId(payload),
    address: typeof payload.to === 'string' ? payload.to : '',
    calldata: cd,
  }
}

function fillSignFromEvent(payload: RpcEvent): SignFill | null {
  if (!payload.method) return null
  return {
    method: payload.method,
    chainId: parseChainId(payload),
    params: payload.rawParams !== undefined
      ? JSON.stringify(payload.rawParams, null, 2)
      : '[]',
  }
}

function App() {
  // ── write decoder state ───────────────────────────────────────────────────
  const [chainId, setChainId] = useState<string>('1')
  const [address, setAddress] = useState<string>('')
  const [calldata, setCalldata] = useState<string>('')
  const [from, setFrom] = useState<string>('')
  const [result, setResult] = useState<DecodeResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [pendingRpcMethod, setPendingRpcMethod] = useState<string | undefined>(undefined)

  // ── sign decoder state ────────────────────────────────────────────────────
  const [signMethod, setSignMethod] = useState<string>('eth_signTypedData_v4')
  const [signChainId, setSignChainId] = useState<string>('1')
  const [signParams, setSignParams] = useState<string>('')
  const [signResult, setSignResult] = useState<SignDecodeResponse | null>(null)
  const [signError, setSignError] = useState<string | null>(null)
  const [signLoading, setSignLoading] = useState(false)

  const [transactions, setTransactions] = useState<EventEntry[]>([])
  const [others, setOthers] = useState<EventEntry[]>([])

  // Subscribe to RPC events broadcast by the backend (the userscript posts
  // here). We DON'T auto-prefill any more — the user explicitly clicks a
  // "Use" button on a captured write/sign event to load it into the form.
  //
  // Defense-in-depth against React StrictMode / HMR: a ref guards against
  // ever opening more than one EventSource, even if this effect re-runs.
  const sseOpenedRef = useRef(false)
  useEffect(() => {
    if (sseOpenedRef.current) return
    sseOpenedRef.current = true
    const es = new EventSource('/api/event/stream')
    es.onmessage = (ev) => {
      try {
        const payload = JSON.parse(ev.data) as RpcEvent
        const cat = categorize(payload.method)
        const isRouteable = cat === 'write'
        const entry: EventEntry = {
          receivedAt: Date.now(),
          payload,
          routedInflight: isRouteable,
        }
        if (cat === 'write' || cat === 'sign') {
          let inserted = true
          setTransactions((prev) => {
            // dApps and userscripts often emit the same write twice in quick
            // succession (estimate-then-send, double-hook, retry, etc).
            // Drop the new entry if the most recent one carries the same
            // method + to + calldata within a short window.
            if (isRecentDuplicate(prev[0], entry, 3000)) {
              inserted = false
              return prev
            }
            return [entry, ...prev].slice(0, TX_LIMIT)
          })

          // Decision 3b: auto-fire /api/route on every captured write event
          // so the verdict card can render without the user clicking [Use].
          // Sign events stay manual — sign evaluation goes through /api/sign
          // (handled separately by the SignDecodeForm).
          if (inserted && isRouteable) {
            void evaluateWriteEvent(payload).then((routed) => {
              setTransactions((prev) =>
                prev.map((e) =>
                  e.receivedAt === entry.receivedAt
                    ? { ...e, routed: routed ?? undefined, routedInflight: false }
                    : e,
                ),
              )
            })
          }
        } else {
          setOthers((prev) => {
            if (isRecentDuplicate(prev[0], entry, 1500)) return prev
            return [entry, ...prev].slice(0, OTHER_LIMIT)
          })
        }
      } catch (e) {
        console.warn('[scopeball] bad event', e)
      }
    }
    es.onerror = () => {
      // EventSource auto-reconnects; surface in console only.
      console.warn('[scopeball] SSE error (will reconnect)')
    }
    return () => {
      es.close()
      sseOpenedRef.current = false
    }
  }, [])

  function loadWriteEventIntoForm(entry: EventEntry) {
    const f = fillWriteFromEvent(entry.payload)
    if (!f) return
    setChainId(f.chainId)
    setAddress(f.address)
    setCalldata(f.calldata)
    // RPC events carry a `from` address (the wallet user) alongside `to`.
    // Populate it so the simulator can recognise the user inside multi-step
    // routing instead of falling back to the zero-address default.
    setFrom(entry.payload.from ?? '')
    setPendingRpcMethod(entry.payload.method)
    requestAnimationFrame(() => {
      document.querySelector('.write-decoder')?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    })
  }

  function loadSignEventIntoForm(entry: EventEntry) {
    const f = fillSignFromEvent(entry.payload)
    if (!f) return
    setSignMethod(f.method)
    setSignChainId(f.chainId)
    setSignParams(f.params)
    requestAnimationFrame(() => {
      document.querySelector('.sign-decoder')?.scrollIntoView({ behavior: 'smooth', block: 'start' })
    })
  }

  async function handleSubmit(req: DecodeRequest) {
    setLoading(true)
    setError(null)
    setResult(null)
    try {
      const r = await decode({ ...req, rpc_method: pendingRpcMethod })
      setResult(r)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }

  // Manual edits invalidate the captured RPC method — once the user types
  // their own calldata we no longer know which RPC it would have come from,
  // so the Etherscan fallback should remain off (strict default).
  function trackedSetChainId(v: string) {
    setChainId(v)
    setPendingRpcMethod(undefined)
  }
  function trackedSetAddress(v: string) {
    setAddress(v)
    setPendingRpcMethod(undefined)
  }
  function trackedSetCalldata(v: string) {
    setCalldata(v)
    setPendingRpcMethod(undefined)
  }

  async function handleSignSubmit(req: SignDecodeRequest) {
    setSignLoading(true)
    setSignError(null)
    setSignResult(null)
    try {
      const r = await decodeSign(req)
      setSignResult(r)
    } catch (e) {
      setSignError(e instanceof Error ? e.message : String(e))
    } finally {
      setSignLoading(false)
    }
  }

  return (
    <div className="app">
      <header className="app-header">
        <div className="app-header-top">
          <div>
            <h1>ABI Resolver</h1>
            <p>Decode arbitrary EVM calldata against a Sourcify-backed signature DB.</p>
          </div>
          <nav className="app-nav">
            <a href="/policy-builder/" className="nav-button">Policy Builder →</a>
          </nav>
        </div>
      </header>
      <main>
        <RpcEventsPanel
          transactions={transactions}
          others={others}
          onClear={() => {
            setTransactions([])
            setOthers([])
          }}
          onLoadWrite={loadWriteEventIntoForm}
          onLoadSign={loadSignEventIntoForm}
        />
        <div className="decoders">
          <section className="write-decoder">
            <h2>Write Decoder</h2>
            <DecodeForm
              chainId={chainId}
              address={address}
              calldata={calldata}
              from={from}
              onChainIdChange={trackedSetChainId}
              onAddressChange={trackedSetAddress}
              onCalldataChange={trackedSetCalldata}
              onFromChange={setFrom}
              onSubmit={handleSubmit}
              loading={loading}
            />
            <DecodeResult result={result} error={error} />
          </section>
          <section className="sign-decoder">
            <h2>Sign Decoder</h2>
            <SignDecodeForm
              method={signMethod}
              chainId={signChainId}
              params={signParams}
              onMethodChange={(v) => { setSignMethod(v); }}
              onChainIdChange={(v) => { setSignChainId(v); }}
              onLoadSample={(sample) => {
                setSignMethod(sample.method)
                setSignChainId(String(sample.chain_id))
                setSignParams(JSON.stringify(sample.params, null, 2))
              }}
              onSubmit={handleSignSubmit}
              loading={signLoading}
            />
            <SignDecodeResult result={signResult} error={signError} />
          </section>
        </div>
      </main>
      <footer>
        <a href="https://sourcify.dev" target="_blank" rel="noreferrer">
          Sourcify
        </a>{' '}
        ·{' '}
        <a href="https://openchain.xyz" target="_blank" rel="noreferrer">
          openchain.xyz
        </a>
      </footer>
    </div>
  )
}

interface EventCardProps {
  entry: EventEntry
  open?: boolean
  onLoadWrite?: (entry: EventEntry) => void
  onLoadSign?: (entry: EventEntry) => void
}

function EventCard({ entry, open, onLoadWrite, onLoadSign }: EventCardProps) {
  const cat = categorize(entry.payload.method)
  const writeDecodable = isWriteDecodable(entry)
  const signDecodable = isSignDecodable(entry)
  return (
    <details open={open} className={`rpc-event cat-${cat}`}>
      <summary>
        <span className={`badge badge-${cat}`}>{CATEGORY_LABEL[cat]}</span>{' '}
        <span className="ts">{formatTime(entry.receivedAt)}</span>{' '}
        <code className="method">{entry.payload.method ?? '?'}</code>{' '}
        <span className="muted">{entry.payload.origin ?? ''}</span>
        {writeDecodable && onLoadWrite ? (
          <button
            type="button"
            className="use-btn"
            onClick={(e) => {
              e.preventDefault()
              e.stopPropagation()
              onLoadWrite(entry)
            }}
            title="Load into Write Decoder"
          >
            Use → Write ↓
          </button>
        ) : null}
        {signDecodable && onLoadSign ? (
          <button
            type="button"
            className="use-btn use-btn-sign"
            onClick={(e) => {
              e.preventDefault()
              e.stopPropagation()
              onLoadSign(entry)
            }}
            title="Load into Sign Decoder"
          >
            Use → Sign ↓
          </button>
        ) : null}
      </summary>
      {(entry.routed !== undefined || entry.routedInflight) && (
        <VerdictCard
          routed={entry.routed ?? undefined}
          inflight={!!entry.routedInflight}
        />
      )}
      <pre className="json">{JSON.stringify(entry.payload, null, 2)}</pre>
    </details>
  )
}

function formatTime(ts: number): string {
  const d = new Date(ts)
  const hh = String(d.getHours()).padStart(2, '0')
  const mm = String(d.getMinutes()).padStart(2, '0')
  const ss = String(d.getSeconds()).padStart(2, '0')
  return `${hh}:${mm}:${ss}`
}

interface RpcEventsPanelProps {
  transactions: EventEntry[]
  others: EventEntry[]
  onClear: () => void
  onLoadWrite: (entry: EventEntry) => void
  onLoadSign: (entry: EventEntry) => void
}

function RpcEventsPanel({
  transactions,
  others,
  onClear,
  onLoadWrite,
  onLoadSign,
}: RpcEventsPanelProps) {
  if (transactions.length === 0 && others.length === 0) {
    return (
      <section className="rpc-events empty">
        <p>
          Waiting for RPC events… open a dApp with the ScopeBall userscript active and
          trigger a transaction (swap / approve / connect).
        </p>
      </section>
    )
  }

  return (
    <section className="rpc-events">
      <header className="rpc-events-header">
        <strong>RPC events</strong>{' '}
        <span className="muted">
          ({transactions.length} tx + {others.length} other)
        </span>
        <button type="button" className="clear-btn" onClick={onClear}>
          Clear
        </button>
      </header>

      {transactions.length > 0 ? (
        <div className="rpc-events-section">
          <h3 className="rpc-events-section-title">
            Transactions <span className="muted">({transactions.length})</span>
          </h3>
          {transactions.map((e, i) => (
            <EventCard
              key={`tx-${e.receivedAt}-${i}`}
              entry={e}
              open={i === 0}
              onLoadWrite={onLoadWrite}
              onLoadSign={onLoadSign}
            />
          ))}
        </div>
      ) : (
        <p className="rpc-events-hint muted">
          No transactions captured yet. Trigger a swap / approve in a dApp to see one
          here with a <strong>Use</strong> button.
        </p>
      )}

      {others.length > 0 ? (
        <details className="rpc-event-history">
          <summary>
            Other events <span className="muted">({others.length})</span>
          </summary>
          {others.map((e, i) => (
            <EventCard key={`o-${e.receivedAt}-${i}`} entry={e} />
          ))}
        </details>
      ) : null}
    </section>
  )
}

export default App
