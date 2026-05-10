import { useEffect, useRef, useState } from 'react'
import { decode } from './api'
import type { DecodeRequest, DecodeResponse } from './api'
import { DecodeForm } from './components/DecodeForm'
import { DecodeResult } from './components/DecodeResult'
import './App.css'

interface RpcEvent {
  method?: string
  origin?: string
  primaryChainId?: string
  chainIds?: string[]
  to?: string
  calldata?: string[]
  [extra: string]: unknown
}

interface EventEntry {
  receivedAt: number
  payload: RpcEvent
}

// Two separate ring buffers so a flood of read/wallet noise can never
// push captured transactions out of the panel.
const TX_LIMIT = 50      // write + sign
const OTHER_LIMIT = 30   // everything else (read/wallet/connect)

type MethodCategory = 'write' | 'sign' | 'connect' | 'wallet' | 'read'

function categorize(method?: string): MethodCategory {
  if (!method) return 'read'
  if (method === 'eth_sendTransaction' || method === 'eth_signTransaction') return 'write'
  if (method === 'personal_sign' || method.startsWith('eth_sign')) return 'sign'
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

// "Decodable" = the form can be filled from this event (we need a target
// address + calldata). Currently that's only eth_sendTransaction-shaped writes.
function isDecodable(entry: EventEntry): boolean {
  const cat = categorize(entry.payload.method)
  if (cat !== 'write') return false
  const cd = entry.payload.calldata
  return Array.isArray(cd) && typeof cd[0] === 'string' && cd[0].length > 0
}

interface FormFill {
  chainId: string
  address: string
  calldata: string
}

function fillFromEvent(payload: RpcEvent): FormFill | null {
  const cd = Array.isArray(payload.calldata) ? payload.calldata[0] : null
  if (!cd) return null
  const cidRaw = payload.primaryChainId ?? payload.chainIds?.[0]
  let chainId = '1'
  if (typeof cidRaw === 'string') {
    const n = cidRaw.startsWith('0x') ? parseInt(cidRaw, 16) : Number(cidRaw)
    if (Number.isFinite(n) && n > 0) chainId = String(n)
  }
  return {
    chainId,
    address: typeof payload.to === 'string' ? payload.to : '',
    calldata: cd,
  }
}

function App() {
  const [chainId, setChainId] = useState<string>('1')
  const [address, setAddress] = useState<string>('')
  const [calldata, setCalldata] = useState<string>('')

  const [result, setResult] = useState<DecodeResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const [transactions, setTransactions] = useState<EventEntry[]>([])
  const [others, setOthers] = useState<EventEntry[]>([])
  // RPC method that produced the currently loaded form data. Forwarded to
  // the backend so it can gate the Etherscan API fallback to write/sign
  // calls only (read/wallet RPCs don't burn the rate-limit budget).
  // Cleared whenever the user edits the form manually.
  const [pendingRpcMethod, setPendingRpcMethod] = useState<string | undefined>(undefined)

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
        const entry: EventEntry = { receivedAt: Date.now(), payload }
        const cat = categorize(payload.method)
        if (cat === 'write' || cat === 'sign') {
          setTransactions((prev) => {
            // dApps and userscripts often emit the same write twice in quick
            // succession (estimate-then-send, double-hook, retry, etc).
            // Drop the new entry if the most recent one carries the same
            // method + to + calldata within a short window.
            if (isRecentDuplicate(prev[0], entry, 3000)) return prev
            return [entry, ...prev].slice(0, TX_LIMIT)
          })
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

  function loadEventIntoForm(entry: EventEntry) {
    const f = fillFromEvent(entry.payload)
    if (!f) return
    setChainId(f.chainId)
    setAddress(f.address)
    setCalldata(f.calldata)
    setPendingRpcMethod(entry.payload.method)
    // Scroll the form into view so the user immediately sees what got loaded.
    requestAnimationFrame(() => {
      document.querySelector('.form')?.scrollIntoView({ behavior: 'smooth', block: 'start' })
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

  return (
    <div className="app">
      <header className="app-header">
        <h1>ABI Resolver</h1>
        <p>Decode arbitrary EVM calldata against a Sourcify-backed signature DB.</p>
      </header>
      <main>
        <RpcEventsPanel
          transactions={transactions}
          others={others}
          onClear={() => {
            setTransactions([])
            setOthers([])
          }}
          onLoad={loadEventIntoForm}
        />
        <DecodeForm
          chainId={chainId}
          address={address}
          calldata={calldata}
          onChainIdChange={trackedSetChainId}
          onAddressChange={trackedSetAddress}
          onCalldataChange={trackedSetCalldata}
          onSubmit={handleSubmit}
          loading={loading}
        />
        <DecodeResult result={result} error={error} />
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
  onLoad?: (entry: EventEntry) => void
}

function EventCard({ entry, open, onLoad }: EventCardProps) {
  const cat = categorize(entry.payload.method)
  const decodable = isDecodable(entry)
  return (
    <details open={open} className={`rpc-event cat-${cat}`}>
      <summary>
        <span className={`badge badge-${cat}`}>{CATEGORY_LABEL[cat]}</span>{' '}
        <span className="ts">{formatTime(entry.receivedAt)}</span>{' '}
        <code className="method">{entry.payload.method ?? '?'}</code>{' '}
        <span className="muted">{entry.payload.origin ?? ''}</span>
        {decodable && onLoad ? (
          <button
            type="button"
            className="use-btn"
            onClick={(e) => {
              e.preventDefault()
              e.stopPropagation()
              onLoad(entry)
            }}
            title="Load this transaction into the decode form"
          >
            Use ↓
          </button>
        ) : null}
      </summary>
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
  onLoad: (entry: EventEntry) => void
}

function RpcEventsPanel({
  transactions,
  others,
  onClear,
  onLoad,
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
              onLoad={onLoad}
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
