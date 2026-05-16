import { useEffect, useState } from 'react'
import type { SignDecodeRequest } from '../api'
import { samplesForMethod, type SignSample } from '../samples/signSamples'

const SIGN_METHODS = [
  'eth_signTypedData_v4',
  'personal_sign',
  'eth_sign',
  'eth_signTransaction',
  'eth_sendUserOperation',
  'wallet_grantPermissions',
]

interface Props {
  method: string
  chainId: string
  params: string
  onMethodChange: (v: string) => void
  onChainIdChange: (v: string) => void
  onLoadSample: (sample: SignSample) => void
  onSubmit: (req: SignDecodeRequest) => void
  loading: boolean
}

interface TypedDataFields {
  signer: string
  typedData: string
}
interface PersonalSignFields { message: string; signer: string }
interface EthSignFields { signer: string; hash: string }
interface SignTxFields { from: string; to: string; data: string; value: string }
interface UserOpFields {
  sender: string; nonce: string; callData: string
  callGasLimit: string; verificationGasLimit: string; preVerificationGas: string
  maxFeePerGas: string; maxPriorityFeePerGas: string; paymasterAndData: string
  entryPoint: string
}
interface GrantPermFields { request: string }

const DEFAULT_TYPED_DATA_JSON = JSON.stringify({ domain: {}, primaryType: '', types: {}, message: {} }, null, 2)
const DEFAULT_GRANT_REQUEST_JSON = JSON.stringify({ signer: '', permissions: [] }, null, 2)

const DEFAULT_TYPED: TypedDataFields = { signer: '', typedData: DEFAULT_TYPED_DATA_JSON }
const DEFAULT_PERSONAL: PersonalSignFields = { message: '', signer: '' }
const DEFAULT_ETH_SIGN: EthSignFields = { signer: '', hash: '' }
const DEFAULT_SIGN_TX: SignTxFields = { from: '', to: '', data: '0x', value: '0x0' }
const DEFAULT_USER_OP: UserOpFields = {
  sender: '', nonce: '0x0', callData: '0x',
  callGasLimit: '0x0', verificationGasLimit: '0x0', preVerificationGas: '0x0',
  maxFeePerGas: '0x0', maxPriorityFeePerGas: '0x0', paymasterAndData: '0x',
  entryPoint: '',
}
const DEFAULT_GRANT: GrantPermFields = { request: DEFAULT_GRANT_REQUEST_JSON }

export function SignDecodeForm({ method, chainId, params, onMethodChange, onChainIdChange, onLoadSample, onSubmit, loading }: Props) {
  const [typedData, setTypedData] = useState<TypedDataFields>(DEFAULT_TYPED)
  const [personalSign, setPersonalSign] = useState<PersonalSignFields>(DEFAULT_PERSONAL)
  const [ethSign, setEthSign] = useState<EthSignFields>(DEFAULT_ETH_SIGN)
  const [signTx, setSignTx] = useState<SignTxFields>(DEFAULT_SIGN_TX)
  const [userOp, setUserOp] = useState<UserOpFields>(DEFAULT_USER_OP)
  const [grantPerm, setGrantPerm] = useState<GrantPermFields>(DEFAULT_GRANT)

  useEffect(() => {
    if (!params.trim()) return
    try {
      const p = JSON.parse(params)
      if (!Array.isArray(p)) return
      distributeParams(method, p)
    } catch { }
  }, [params, method])

  function distributeParams(m: string, p: unknown[]) {
    switch (m) {
      case 'eth_signTypedData_v4': {
        const signer = typeof p[0] === 'string' ? p[0] : ''
        let td: unknown = {}
        if (typeof p[1] === 'string') {
          try { td = JSON.parse(p[1]) } catch { td = p[1] }
        } else if (p[1] && typeof p[1] === 'object') {
          td = p[1]
        }
        setTypedData({
          signer,
          typedData: JSON.stringify(td, null, 2),
        })
        break
      }
      case 'personal_sign':
        setPersonalSign({
          message: typeof p[0] === 'string' ? p[0] : '',
          signer: typeof p[1] === 'string' ? p[1] : '',
        })
        break
      case 'eth_sign':
        setEthSign({
          signer: typeof p[0] === 'string' ? p[0] : '',
          hash: typeof p[1] === 'string' ? p[1] : '',
        })
        break
      case 'eth_signTransaction': {
        const tx = (p[0] && typeof p[0] === 'object' ? p[0] : {}) as Record<string, unknown>
        setSignTx({
          from: typeof tx.from === 'string' ? tx.from : '',
          to: typeof tx.to === 'string' ? tx.to : '',
          data: typeof tx.data === 'string' ? tx.data : '0x',
          value: typeof tx.value === 'string' ? tx.value : '0x0',
        })
        break
      }
      case 'eth_sendUserOperation': {
        const uo = (p[0] && typeof p[0] === 'object' ? p[0] : {}) as Record<string, unknown>
        setUserOp({
          sender: typeof uo.sender === 'string' ? uo.sender : '',
          nonce: typeof uo.nonce === 'string' ? uo.nonce : '0x0',
          callData: typeof uo.callData === 'string' ? uo.callData : '0x',
          callGasLimit: typeof uo.callGasLimit === 'string' ? uo.callGasLimit : '0x0',
          verificationGasLimit: typeof uo.verificationGasLimit === 'string' ? uo.verificationGasLimit : '0x0',
          preVerificationGas: typeof uo.preVerificationGas === 'string' ? uo.preVerificationGas : '0x0',
          maxFeePerGas: typeof uo.maxFeePerGas === 'string' ? uo.maxFeePerGas : '0x0',
          maxPriorityFeePerGas: typeof uo.maxPriorityFeePerGas === 'string' ? uo.maxPriorityFeePerGas : '0x0',
          paymasterAndData: typeof uo.paymasterAndData === 'string' ? uo.paymasterAndData : '0x',
          entryPoint: typeof p[1] === 'string' ? p[1] : '',
        })
        break
      }
      case 'wallet_grantPermissions': {
        const req = p[0] && typeof p[0] === 'object' ? p[0] : {}
        setGrantPerm({ request: JSON.stringify(req, null, 2) })
        break
      }
    }
  }

  function buildParams(): unknown {
    switch (method) {
      case 'eth_signTypedData_v4': {
        let parsed
        try { parsed = JSON.parse(typedData.typedData) } catch { throw new Error('typed data is not valid JSON') }
        return [typedData.signer, JSON.stringify(parsed)]
      }
      case 'personal_sign':
        return [personalSign.message, personalSign.signer]
      case 'eth_sign':
        return [ethSign.signer, ethSign.hash]
      case 'eth_signTransaction':
        return [{ from: signTx.from, to: signTx.to, data: signTx.data, value: signTx.value, chainId: Number(chainId) }]
      case 'eth_sendUserOperation':
        return [{
          sender: userOp.sender, nonce: userOp.nonce, callData: userOp.callData,
          callGasLimit: userOp.callGasLimit, verificationGasLimit: userOp.verificationGasLimit,
          preVerificationGas: userOp.preVerificationGas, maxFeePerGas: userOp.maxFeePerGas,
          maxPriorityFeePerGas: userOp.maxPriorityFeePerGas, paymasterAndData: userOp.paymasterAndData,
        }, userOp.entryPoint]
      case 'wallet_grantPermissions': {
        let request
        try { request = JSON.parse(grantPerm.request) } catch { throw new Error('request is not valid JSON') }
        return [request]
      }
      default:
        return []
    }
  }

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    const id = Number(chainId)
    if (!Number.isFinite(id) || id <= 0) {
      alert('chain_id must be a positive number')
      return
    }
    let builtParams: unknown
    try {
      builtParams = buildParams()
    } catch (err) {
      alert(err instanceof Error ? err.message : 'Invalid input')
      return
    }
    onSubmit({ method, params: builtParams, chain_id: id })
  }

  function td(label: string, value: string, onChange: (v: string) => void, mono = true, rows?: number) {
    return (
      <div className="row">
        <label>
          <span>{label}</span>
          {rows !== undefined
            ? <textarea rows={rows} value={value} onChange={(e) => onChange(e.target.value)} disabled={loading} spellCheck={false} style={mono ? { fontFamily: 'ui-monospace, monospace' } : undefined} />
            : <input type="text" value={value} onChange={(e) => onChange(e.target.value)} disabled={loading} spellCheck={false} />
          }
        </label>
      </div>
    )
  }

  function renderFields() {
    switch (method) {
      case 'eth_signTypedData_v4':
        return (
          <>
            {td('Signer', typedData.signer, (v) => setTypedData((s) => ({ ...s, signer: v })))}
            {td('Typed Data (JSON)', typedData.typedData, (v) => setTypedData((s) => ({ ...s, typedData: v })), true, 14)}
          </>
        )
      case 'personal_sign':
        return (
          <>
            {td('Message (hex or text)', personalSign.message, (v) => setPersonalSign((s) => ({ ...s, message: v })), true, 4)}
            {td('Signer', personalSign.signer, (v) => setPersonalSign((s) => ({ ...s, signer: v })))}
          </>
        )
      case 'eth_sign':
        return (
          <>
            {td('Signer', ethSign.signer, (v) => setEthSign((s) => ({ ...s, signer: v })))}
            {td('Hash (32-byte hex)', ethSign.hash, (v) => setEthSign((s) => ({ ...s, hash: v })))}
          </>
        )
      case 'eth_signTransaction':
        return (
          <>
            {td('From', signTx.from, (v) => setSignTx((s) => ({ ...s, from: v })))}
            {td('To', signTx.to, (v) => setSignTx((s) => ({ ...s, to: v })))}
            {td('Data', signTx.data, (v) => setSignTx((s) => ({ ...s, data: v })), true, 6)}
            {td('Value', signTx.value, (v) => setSignTx((s) => ({ ...s, value: v })))}
          </>
        )
      case 'eth_sendUserOperation':
        return (
          <>
            {td('Sender', userOp.sender, (v) => setUserOp((s) => ({ ...s, sender: v })))}
            {td('Nonce', userOp.nonce, (v) => setUserOp((s) => ({ ...s, nonce: v })))}
            {td('CallData', userOp.callData, (v) => setUserOp((s) => ({ ...s, callData: v })))}
            {td('EntryPoint', userOp.entryPoint, (v) => setUserOp((s) => ({ ...s, entryPoint: v })))}
            <details className="sign-form-gas">
              <summary>Gas fields</summary>
              {td('callGasLimit', userOp.callGasLimit, (v) => setUserOp((s) => ({ ...s, callGasLimit: v })))}
              {td('verificationGasLimit', userOp.verificationGasLimit, (v) => setUserOp((s) => ({ ...s, verificationGasLimit: v })))}
              {td('preVerificationGas', userOp.preVerificationGas, (v) => setUserOp((s) => ({ ...s, preVerificationGas: v })))}
              {td('maxFeePerGas', userOp.maxFeePerGas, (v) => setUserOp((s) => ({ ...s, maxFeePerGas: v })))}
              {td('maxPriorityFeePerGas', userOp.maxPriorityFeePerGas, (v) => setUserOp((s) => ({ ...s, maxPriorityFeePerGas: v })))}
              {td('paymasterAndData', userOp.paymasterAndData, (v) => setUserOp((s) => ({ ...s, paymasterAndData: v })))}
            </details>
          </>
        )
      case 'wallet_grantPermissions':
        return (
          <>
            {td('Request (JSON)', grantPerm.request, (v) => setGrantPerm((s) => ({ ...s, request: v })), true, 14)}
          </>
        )
      default:
        return null
    }
  }

  return (
    <form onSubmit={handleSubmit} className="form sign-form">
      <div className="row">
        <label>
          <span>Method</span>
          <select
            value={SIGN_METHODS.includes(method) ? method : '__custom__'}
            onChange={(e) => { if (e.target.value !== '__custom__') onMethodChange(e.target.value) }}
            disabled={loading}
          >
            {SIGN_METHODS.map((m) => <option key={m} value={m}>{m}</option>)}
            {!SIGN_METHODS.includes(method) && (
              <option value="__custom__">{method} (custom)</option>
            )}
          </select>
        </label>
      </div>
      <div className="row">
        <label>
          <span>Chain ID</span>
          <input type="number" min={1} value={chainId} onChange={(e) => onChainIdChange(e.target.value)} disabled={loading} />
        </label>
      </div>
      {renderFields()}
      <div className="row submit-row">
        <button type="submit" disabled={loading}>
          {loading ? 'Decoding…' : 'Decode Sign'}
        </button>
      </div>
      <SampleRow method={method} onLoadSample={onLoadSample} disabled={loading} />
    </form>
  )
}

function SampleRow({
  method,
  onLoadSample,
  disabled,
}: {
  method: string
  onLoadSample: (sample: SignSample) => void
  disabled: boolean
}) {
  const samples = samplesForMethod(method)
  if (samples.length === 0) return null
  return (
    <div className="samples">
      <span>Try a sample:</span>
      {samples.map((s, i) => (
        <button
          key={i}
          type="button"
          className="sample"
          disabled={disabled}
          onClick={() => onLoadSample(s)}
          title={s.notes ?? s.label}
        >
          {s.label}
        </button>
      ))}
    </div>
  )
}
