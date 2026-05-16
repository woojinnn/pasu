import type { DecodeRequest } from '../api'

interface Sample {
  label: string
  request: DecodeRequest
}

const SAMPLES: Sample[] = [
  {
    label: 'USDC approve(V3 SwapRouter, 100 USDC)',
    request: {
      chain_id: 1,
      address: '0xA0b86991C6218b36c1d19D4a2e9Eb0cE3606eB48',
      calldata:
        '0x095ea7b3000000000000000000000000e592427a0aece92de3edee1f18e0157c058615640000000000000000000000000000000000000000000000000000000005f5e100',
    },
  },
  {
    label: 'Aave V3 Pool — withdraw',
    request: {
      chain_id: 1,
      address: '0x87870bca3f3fd6335c3f4ce8392d69350b4fa4e2',
      calldata:
        '0x69328dec000000000000000000000000a0b86991c6218b36c1d19d4a2e9eb0ce3606eb480000000000000000000000000000000000000000000000000000000000000064000000000000000000000000e7f525dd1bc6d748ae4d7f21d31e54741e05e110',
    },
  },
  {
    label: 'Universal Router — execute(deadline)',
    request: {
      chain_id: 1,
      address: '0x66a9893cc07d91d95644aedd05d03f95e1dba8af',
      calldata:
        '0x3593564c000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000069f9b8c300000000000000000000000000000000000000000000000000000000000000020b000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000e35fa931a00000000000000000000000000000000000000000000000000000000000000000140000000000000000000000000b8bcee146c822f23963cb63b7d6dc87547ffc895000000000000000000000000000000000000000000000000000e35fa931a0000000000000000000000000000000000000000000000000000000000000090321700000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000120000000000000000000000000000000000000000000000000000000000000002bc02aaa39b223fe8d0a0e5c4f27ead9083c756cc20001f4a0b86991c6218b36c1d19d4a2e9eb0ce3606eb4800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000',
    },
  },
]

interface Props {
  chainId: string
  address: string
  calldata: string
  from: string
  onChainIdChange: (v: string) => void
  onAddressChange: (v: string) => void
  onCalldataChange: (v: string) => void
  onFromChange: (v: string) => void
  onSubmit: (req: DecodeRequest) => void
  loading: boolean
}

export function DecodeForm({
  chainId,
  address,
  calldata,
  from,
  onChainIdChange,
  onAddressChange,
  onCalldataChange,
  onFromChange,
  onSubmit,
  loading,
}: Props) {
  function loadSample(s: Sample) {
    onChainIdChange(String(s.request.chain_id))
    onAddressChange(s.request.address)
    onCalldataChange(s.request.calldata)
    onFromChange(s.request.from ?? '')
  }

  function submit(e: React.FormEvent) {
    e.preventDefault()
    const id = Number(chainId)
    if (!Number.isFinite(id) || id <= 0) {
      alert('chain_id must be a positive number')
      return
    }
    const trimmedFrom = from.trim()
    onSubmit({
      chain_id: id,
      address: address.trim(),
      calldata: calldata.trim(),
      // Only include `from` when non-empty so backend keeps its
      // zero-address default for callers who don't care.
      ...(trimmedFrom ? { from: trimmedFrom } : {}),
    })
  }

  return (
    <form onSubmit={submit} className="form">
      <div className="row">
        <label>
          <span>Chain ID</span>
          <input
            type="number"
            min={1}
            value={chainId}
            onChange={(e) => onChainIdChange(e.target.value)}
            disabled={loading}
          />
        </label>
      </div>
      <div className="row">
        <label>
          <span>Contract address (to)</span>
          <input
            type="text"
            placeholder="0x…"
            value={address}
            onChange={(e) => onAddressChange(e.target.value)}
            disabled={loading}
            spellCheck={false}
          />
        </label>
      </div>
      <div className="row">
        <label>
          <span>From address (optional — wallet user)</span>
          <input
            type="text"
            placeholder="0x…  (leave blank to use zero address)"
            value={from}
            onChange={(e) => onFromChange(e.target.value)}
            disabled={loading}
            spellCheck={false}
          />
        </label>
      </div>
      <div className="row">
        <label>
          <span>Calldata (hex, with or without 0x)</span>
          <textarea
            rows={6}
            placeholder="0x095ea7b3…"
            value={calldata}
            onChange={(e) => onCalldataChange(e.target.value)}
            disabled={loading}
            spellCheck={false}
          />
        </label>
      </div>

      <div className="row submit-row">
        <button type="submit" disabled={loading}>
          {loading ? 'Decoding…' : 'Decode'}
        </button>
        <div className="samples">
          <span>Try a sample:</span>
          {SAMPLES.map((s) => (
            <button
              key={s.label}
              type="button"
              className="sample"
              onClick={() => loadSample(s)}
              disabled={loading}
            >
              {s.label}
            </button>
          ))}
        </div>
      </div>
    </form>
  )
}
