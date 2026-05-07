import { useState } from 'react'
import { decode } from './api'
import type { DecodeRequest, DecodeResponse } from './api'
import { DecodeForm } from './components/DecodeForm'
import { DecodeResult } from './components/DecodeResult'
import './App.css'

function App() {
  const [result, setResult] = useState<DecodeResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  async function handleSubmit(req: DecodeRequest) {
    setLoading(true)
    setError(null)
    setResult(null)
    try {
      const r = await decode(req)
      setResult(r)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="app">
      <header className="app-header">
        <h1>ABI Resolver</h1>
        <p>Decode arbitrary EVM calldata against a Sourcify-backed signature DB.</p>
      </header>
      <main>
        <DecodeForm onSubmit={handleSubmit} loading={loading} />
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

export default App
