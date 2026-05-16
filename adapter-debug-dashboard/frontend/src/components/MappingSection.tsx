import type { MappingActionEnvelope, MappingRoot } from '../api'

interface Props {
  root: MappingRoot
  /** Optional heading override — defaults to "Schema mapping". */
  title?: string
}

/**
 * Renders a schema-shaped `RootRequest` as a human-readable block.
 * Used by both `DecodeResult` (write side) and `SignDecodeResult` (sign
 * side) so the visual shape of the mapping section stays identical
 * regardless of which endpoint produced it.
 */
export function MappingSection({ root, title = 'Schema mapping' }: Props) {
  return (
    <div className="mapping">
      <header>
        <h3>{title}</h3>
        <span className="schema-version">schema v{root.schemaVersion}</span>
      </header>
      <table className="args">
        <tbody>
          <tr>
            <td><code>requestKind</code></td>
            <td className="value"><code>{root.requestKind}</code></td>
          </tr>
          <tr>
            <td><code>chainId</code></td>
            <td className="value"><code>{root.chainId}</code></td>
          </tr>
          <tr>
            <td><code>from</code></td>
            <td className="value"><code>{root.from}</code></td>
          </tr>
          <tr>
            <td><code>to</code></td>
            <td className="value"><code>{root.to}</code></td>
          </tr>
          <tr>
            <td><code>value</code></td>
            <td className="value"><code>{root.value}</code></td>
          </tr>
          <tr>
            <td><code>selector</code></td>
            <td className="value"><code>{root.selector}</code></td>
          </tr>
          {root.blockTimestamp !== undefined && (
            <tr>
              <td><code>blockTimestamp</code></td>
              <td className="value"><code>{root.blockTimestamp}</code></td>
            </tr>
          )}
        </tbody>
      </table>
      {root.protocol && (
        <p className="protocol">
          <strong>Protocol:</strong>{' '}
          <code>
            {root.protocol.name}
            {root.protocol.version ? ` ${root.protocol.version}` : ''}
            {root.protocol.component ? ` / ${root.protocol.component}` : ''}
          </code>
        </p>
      )}
      <div className="actions">
        {root.actions.map((env, i) => (
          <MappingAction key={i} env={env} index={i} />
        ))}
      </div>
      <details className="raw">
        <summary>Mapping JSON</summary>
        <pre>{JSON.stringify(root, null, 2)}</pre>
      </details>
    </div>
  )
}

function MappingAction({ env, index }: { env: MappingActionEnvelope; index: number }) {
  const f = env.fields as Record<string, unknown>
  const rows: Array<[string, unknown]> = []
  for (const [k, v] of Object.entries(f)) {
    if (k === '_kind') continue
    rows.push([k, v])
  }
  return (
    <div className="action">
      <header>
        <h4>
          <span className="idx">#{index}</span> {env.action}{' '}
          <span className="category">[{env.category}]</span>
        </h4>
      </header>
      <dl className="fields">
        {rows.map(([k, v], i) => (
          <div key={i} className="field-row">
            <dt>
              <code>{k}</code>
            </dt>
            <dd>{renderFieldValue(v)}</dd>
          </div>
        ))}
      </dl>
    </div>
  )
}

/**
 * Render an individual field value. Primitives → inline `<code>`; nested
 * objects (e.g., `tokenIn`, `validity`, `domain`) → pretty-printed JSON in a
 * `<pre>` block so users can see the structure.
 */
function renderFieldValue(v: unknown): React.ReactNode {
  if (v === null || v === undefined) return <code className="value">—</code>
  if (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') {
    return <code className="value">{String(v)}</code>
  }
  return <pre className="field-json">{JSON.stringify(v, null, 2)}</pre>
}
