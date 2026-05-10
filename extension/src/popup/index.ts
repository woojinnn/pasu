import Browser from 'webextension-polyfill';
import './styles.css';

interface CatalogPolicy {
  id: string;
  rules: { severity: 'deny' | 'warn' | 'unknown'; reason: string }[];
  dominantSeverity: 'deny' | 'warn' | 'unknown';
  sourceLabel: string;
}
interface Catalog {
  policies: CatalogPolicy[];
  enabled: string[];
  applied: string[];
}
type ApplyResponse =
  | { ok: true }
  | { ok: false; error: { kind: string; message: string } };
type CatalogResponse =
  | { ok: true; data: Catalog }
  | { ok: false; error: { kind: string; message: string } };

const state: {
  catalog: Catalog | null;
  searchTerm: string;
  status: 'idle' | 'applying' | 'error';
  errorText: string;
  expanded: Set<string>;
} = { catalog: null, searchTerm: '', status: 'idle', errorText: '', expanded: new Set() };

async function fetchCatalog(): Promise<Catalog> {
  // Guard against a wedged service worker — if no response arrives in
  // 5 s, surface a visible error instead of staying on "Loading…".
  const timeout = new Promise<never>((_, reject) =>
    setTimeout(
      () => reject(new Error('no response from service worker (timeout 5s)')),
      5000,
    ),
  );
  const res = (await Promise.race([
    Browser.runtime.sendMessage({ type: 'policy-catalog' }),
    timeout,
  ])) as CatalogResponse | undefined;
  if (!res) throw new Error('empty response from service worker');
  if (!res.ok) throw new Error(`${res.error.kind}: ${res.error.message}`);
  return res.data;
}

async function postSetEnabledIds(ids: string[]): Promise<ApplyResponse> {
  return (await Browser.runtime.sendMessage({
    type: 'set-enabled-ids',
    ids,
  })) as ApplyResponse;
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Partial<{ class: string; text: string; type: string; placeholder: string }> = {},
  children: (HTMLElement | string)[] = [],
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (attrs.class) node.className = attrs.class;
  if (attrs.text !== undefined) node.textContent = attrs.text;
  if (attrs.type && 'type' in node) (node as unknown as HTMLInputElement).type = attrs.type;
  if (attrs.placeholder && 'placeholder' in node)
    (node as unknown as HTMLInputElement).placeholder = attrs.placeholder;
  for (const c of children) {
    node.appendChild(typeof c === 'string' ? document.createTextNode(c) : c);
  }
  return node;
}

function badge(severity: 'deny' | 'warn' | 'unknown'): HTMLSpanElement {
  return el('span', { class: `badge ${severity}`, text: severity });
}

function distinctReasons(p: CatalogPolicy): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const r of p.rules) {
    if (!seen.has(r.reason)) {
      seen.add(r.reason);
      out.push(r.reason);
    }
  }
  return out;
}

function matchesSearch(p: CatalogPolicy, term: string): boolean {
  if (!term) return true;
  const t = term.toLowerCase();
  if (p.id.toLowerCase().includes(t)) return true;
  return p.rules.some((r) => r.reason.toLowerCase().includes(t));
}

async function applyIds(ids: string[]): Promise<void> {
  state.status = 'applying';
  render();
  const result = await postSetEnabledIds(ids);
  if (result.ok) {
    state.status = 'idle';
    state.errorText = '';
  } else {
    state.status = 'error';
    state.errorText = `${result.error.kind}: ${result.error.message}`;
  }
  try {
    state.catalog = await fetchCatalog();
  } catch (err) {
    state.status = 'error';
    state.errorText = `refresh failed: ${String(err)}`;
  }
  render();
}

function renderRow(p: CatalogPolicy, enabledSet: Set<string>): HTMLDivElement {
  const reasons = distinctReasons(p);
  const reasonText = reasons[0] ?? '(no reason annotation)';
  const moreCount = reasons.length - 1;
  const isExpanded = state.expanded.has(p.id);

  const checkbox = el('input', { type: 'checkbox' }) as HTMLInputElement;
  checkbox.checked = enabledSet.has(p.id);
  checkbox.setAttribute('aria-label', `Enable policy ${p.id}`);
  checkbox.addEventListener('change', () => {
    const next = new Set(enabledSet);
    if (checkbox.checked) next.add(p.id);
    else next.delete(p.id);
    void applyIds([...next]);
  });

  const caret = el('span', {
    class: `caret ${isExpanded ? 'open' : ''}`,
    text: isExpanded ? '▾' : '▸',
  });

  const idLine = el('div', { class: 'id' }, [caret, el('span', { text: p.id })]);

  const meta = el('div', { class: 'meta' }, [
    idLine,
    el('div', { class: 'reason' }, [
      badge(p.dominantSeverity),
      reasonText,
      ...(moreCount > 0
        ? [el('span', { class: 'chip-more', text: `+${moreCount} more` })]
        : []),
    ]),
  ]);
  meta.style.cursor = 'pointer';
  meta.setAttribute('role', 'button');
  meta.setAttribute('aria-expanded', isExpanded ? 'true' : 'false');
  meta.setAttribute('aria-label', `Toggle details for ${p.id}`);
  meta.addEventListener('click', () => {
    if (state.expanded.has(p.id)) state.expanded.delete(p.id);
    else state.expanded.add(p.id);
    render();
  });

  const onlyBtn = el('button', { class: 'only', text: 'Only this' });
  onlyBtn.setAttribute('aria-label', `Enable only ${p.id}`);
  onlyBtn.addEventListener('click', (ev) => {
    ev.stopPropagation();
    void applyIds([p.id]);
  });

  const row = el('div', { class: 'row' }, [checkbox, meta, onlyBtn]);
  if (isExpanded) {
    const ruleNodes = p.rules.map((r) =>
      el('div', { class: 'rule' }, [
        badge(r.severity),
        el('span', { class: 'rule-reason', text: r.reason }),
      ]),
    );
    const details = el('div', { class: 'details' }, [
      el('div', { class: 'detail-source' }, [
        el('span', { class: 'detail-label', text: 'Source: ' }),
        el('span', { text: p.sourceLabel }),
      ]),
      el('div', { class: 'detail-rules' }, [
        el('span', {
          class: 'detail-label',
          text: `Rules (${p.rules.length}):`,
        }),
        ...ruleNodes,
      ]),
    ]);
    return el('div', { class: 'row-wrap' }, [row, details]);
  }
  return el('div', { class: 'row-wrap' }, [row]);
}

function render(): void {
  const root = document.getElementById('root');
  if (!root) return;
  root.replaceChildren();

  if (!state.catalog) {
    const placeholder = state.status === 'error'
      ? `Error: ${state.errorText}`
      : 'Loading…';
    root.appendChild(el('main', {}, [el('p', { text: placeholder })]));
    if (state.status === 'error') {
      root.appendChild(
        el('footer', {}, [el('span', { class: 'status error', text: 'Failed to load catalog' })]),
      );
    }
    return;
  }

  const c = state.catalog;
  const enabledSet = new Set(c.enabled);
  const total = c.policies.length;
  const enabledCount = c.enabled.length;

  // Header
  const titleRow = el('div', { class: 'title-row' }, [
    el('h1', { text: `${enabledCount} of ${total} enabled` }),
    el('div', { class: 'actions' }, [
      (() => {
        const b = el('button', { text: 'Enable all' });
        b.addEventListener('click', () => void applyIds(c.policies.map((p) => p.id)));
        return b;
      })(),
      (() => {
        const b = el('button', { text: 'Disable all' });
        b.addEventListener('click', () => void applyIds([]));
        return b;
      })(),
    ]),
  ]);

  const search = el('input', {
    class: 'search',
    type: 'text',
    placeholder: 'Search by id or reason',
  }) as HTMLInputElement;
  search.value = state.searchTerm;
  search.addEventListener('input', () => {
    state.searchTerm = search.value;
    render();
    const newSearch = document.querySelector<HTMLInputElement>('.search');
    if (newSearch) {
      newSearch.focus();
      const len = newSearch.value.length;
      newSearch.setSelectionRange(len, len);
    }
  });

  const headerChildren: (HTMLElement | string)[] = [titleRow, search];
  if (total > 0 && enabledCount === 0) {
    headerChildren.push(
      el('div', {
        class: 'banner',
        text:
          'All policies disabled — every Cedar verdict will pass; the orchestrator may still warn on unsupported request paths.',
      }),
    );
  }
  root.appendChild(el('header', {}, headerChildren));

  // Body
  const main = el('main');
  const groups = new Map<string, CatalogPolicy[]>();
  for (const p of c.policies) {
    if (!matchesSearch(p, state.searchTerm)) continue;
    if (!groups.has(p.sourceLabel)) groups.set(p.sourceLabel, []);
    groups.get(p.sourceLabel)!.push(p);
  }
  for (const [label, items] of groups) {
    const section = el('section', { class: 'section' }, [
      el('h2', { text: label }),
      ...items.map((p) => renderRow(p, enabledSet)),
    ]);
    main.appendChild(section);
  }
  if (groups.size === 0) {
    main.appendChild(el('p', { text: 'No matches.' }));
  }
  root.appendChild(main);

  // Footer
  let statusText = 'Up to date';
  let statusClass = 'status';
  if (state.status === 'applying') statusText = 'Reinstalling…';
  if (state.status === 'error') {
    statusText = `Error: ${state.errorText}`;
    statusClass = 'status error';
  } else if (
    [...enabledSet].sort().join(',') !== [...c.applied].sort().join(',')
  ) {
    statusText = 'Reinstalling…';
  }
  root.appendChild(el('footer', {}, [el('span', { class: statusClass, text: statusText })]));
}

// Show the "Loading…" shell synchronously so the popup never appears
// empty while the first message round-trip to the SW is in flight.
render();

void (async () => {
  try {
    state.catalog = await fetchCatalog();
  } catch (err) {
    state.status = 'error';
    state.errorText = String(err);
  }
  render();
})();
