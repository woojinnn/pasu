import type { SignDecodeResponse } from '../api'

interface Props {
  result: SignDecodeResponse | null
  error: string | null
}

export function SignDecodeResult({ result, error }: Props) {
  if (error) {
    return (
      <section className="result error">
        <h2>Sign Decode Error</h2>
        <pre>{error}</pre>
      </section>
    )
  }
  if (!result) {
    return (
      <section className="result placeholder">
        <p>Submit a sign decode request to see results here.</p>
      </section>
    )
  }

  return (
    <section className="result">
      <header>
        <h3>
          <span className="badge badge-sign">SIGN</span>{' '}
          <code>{result.method}</code>
        </h3>
      </header>
      <table className="args">
        <tbody>
          <tr>
            <td><code>signer</code></td>
            <td className="value"><code>{result.signer || '(unknown)'}</code></td>
          </tr>
          <tr>
            <td><code>chain_id</code></td>
            <td className="value"><code>{result.chain_id}</code></td>
          </tr>
          <tr>
            <td><code>payload kind</code></td>
            <td className="value"><code>{result.payload.kind}</code></td>
          </tr>
        </tbody>
      </table>
      <PayloadDetail payload={result.payload} />
      <details className="raw">
        <summary>Raw JSON</summary>
        <pre>{JSON.stringify(result, null, 2)}</pre>
      </details>
    </section>
  )
}

function PayloadDetail({ payload }: { payload: SignDecodeResponse['payload'] }) {
  switch (payload.kind) {
    case 'typed_data': {
      const data = payload.data as Record<string, unknown> | undefined
      return (
        <div className="sign-payload">
          <h4>Typed Data</h4>
          {Boolean(data?.primaryType) && (
            <p><strong>primaryType:</strong> <code>{String(data?.primaryType)}</code></p>
          )}
          {Boolean(data?.domain) && (
            <details open>
              <summary>domain</summary>
              <pre>{JSON.stringify(data?.domain, null, 2)}</pre>
            </details>
          )}
          {Boolean(data?.message) && (
            <details open>
              <summary>message</summary>
              <pre>{JSON.stringify(data?.message, null, 2)}</pre>
            </details>
          )}
        </div>
      )
    }
    case 'raw_message':
      return (
        <div className="sign-payload">
          <h4>Message</h4>
          <code className="value">{String(payload.message ?? '')}</code>
        </div>
      )
    case 'raw_hash':
      return (
        <div className="sign-payload">
          <h4>Hash</h4>
          <code className="value">{String(payload.hash ?? '')}</code>
        </div>
      )
    case 'transaction':
      return (
        <div className="sign-payload">
          <h4>Transaction</h4>
          <pre>{JSON.stringify(payload.tx, null, 2)}</pre>
        </div>
      )
    case 'user_operation':
      return (
        <div className="sign-payload">
          <h4>UserOperation</h4>
          {Boolean(payload.entry_point) && (
            <p><strong>entryPoint:</strong> <code>{String(payload.entry_point)}</code></p>
          )}
          <pre>{JSON.stringify(payload.user_op, null, 2)}</pre>
        </div>
      )
    case 'permission_request':
      return (
        <div className="sign-payload">
          <h4>Permission Request</h4>
          <pre>{JSON.stringify(payload.request, null, 2)}</pre>
        </div>
      )
    default:
      return (
        <div className="sign-payload">
          <pre>{JSON.stringify(payload, null, 2)}</pre>
        </div>
      )
  }
}
