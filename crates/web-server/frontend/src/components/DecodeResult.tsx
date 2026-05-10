import type { DecodeResponse } from '../api'

interface Props {
  result: DecodeResponse | null
  error: string | null
}

const SOURCE_LABEL: Record<string, string> = {
  sourcify_curated: 'Sourcify (curated bundle)',
  sourcify_db: 'Sourcify DB dump',
  openchain: 'openchain (selector match)',
  ur_command: 'Universal Router opcode',
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

  return (
    <section className="result">
      <DecodeNode node={result} depth={0} />
      <details className="raw">
        <summary>Raw JSON</summary>
        <pre>{JSON.stringify(result, null, 2)}</pre>
      </details>
    </section>
  )
}

interface NodeProps {
  node: DecodeResponse
  depth: number
}

/**
 * Render one decode node. When the response includes `children` (Cat A
 * recursive multicall) the component recurses for each child with `depth+1`
 * so nested multicalls indent visually.
 */
function DecodeNode({ node, depth }: NodeProps) {
  const indent = depth > 0 ? { marginLeft: `${depth * 1.25}rem` } : undefined

  if (node.outcome === 'not_found') {
    return (
      <div className="resolved not-found" style={indent}>
        <h3>Not found</h3>
        <p>{node.message}</p>
        <p>
          <strong>Selector:</strong> <code>{node.selector}</code>
        </p>
      </div>
    )
  }

  return (
    <div className="resolved" style={indent}>
      <header>
        <h3>{node.function_name}</h3>
        <span className="source">{SOURCE_LABEL[node.source] ?? node.source}</span>
      </header>
      <p className="signature">
        <code>{node.signature}</code>
      </p>
      <p className="selector">
        <strong>Selector:</strong> <code>{node.selector}</code>
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
          {node.args.map((a, i) => (
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
      {node.children && node.children.length > 0 && (
        <div className="children">
          <h4>
            Sub-calls <span className="count">({node.children.length})</span>
          </h4>
          {node.children.map((child, i) => (
            <DecodeNode key={i} node={child} depth={depth + 1} />
          ))}
        </div>
      )}
    </div>
  )
}
