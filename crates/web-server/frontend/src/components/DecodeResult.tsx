import type { DecodeResponse } from '../api'

interface Props {
  result: DecodeResponse | null
  error: string | null
}

const SOURCE_LABEL: Record<string, string> = {
  sourcify_curated: 'Sourcify (curated bundle)',
  sourcify_db: 'Sourcify DB dump',
  openchain: 'openchain (selector match)',
}

export function DecodeResult({ result, error }: Props) {
  if (error) {
    return (
      <section className="result error">
        <h2>Error</h2>
        <pre>{error}</pre>
      </section>
    )
  }
  if (!result) {
    return (
      <section className="result placeholder">
        <p>Submit a decode request to see results here.</p>
      </section>
    )
  }
  if (result.outcome === 'not_found') {
    return (
      <section className="result not-found">
        <h2>Not found</h2>
        <p>{result.message}</p>
        <p>
          <strong>Selector:</strong> <code>{result.selector}</code>
        </p>
      </section>
    )
  }

  return (
    <section className="result resolved">
      <header>
        <h2>{result.function_name}</h2>
        <span className="source">{SOURCE_LABEL[result.source] ?? result.source}</span>
      </header>
      <p className="signature">
        <code>{result.signature}</code>
      </p>
      <p className="selector">
        <strong>Selector:</strong> <code>{result.selector}</code>
      </p>
      <table className="args">
        <thead>
          <tr>
            <th>Name</th>
            <th>Type</th>
            <th>Value</th>
          </tr>
        </thead>
        <tbody>
          {result.args.map((a, i) => (
            <tr key={i}>
              <td>
                <code>{a.name}</code>
              </td>
              <td>
                <code>{a.sol_type}</code>
              </td>
              <td className="value">
                <code>{a.value}</code>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <details className="raw">
        <summary>Raw JSON</summary>
        <pre>{JSON.stringify(result, null, 2)}</pre>
      </details>
    </section>
  )
}
