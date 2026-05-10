export type Severity = 'deny' | 'warn' | 'unknown';

export interface Rule {
  severity: Severity;
  reason: string;
}

export interface PolicyMeta {
  rules: Rule[];
  dominantSeverity: Severity;
}

const SEVERITY_RE = /@severity\(\s*"([^"]+)"\s*\)/;
const REASON_RE = /@reason\(\s*"((?:[^"\\]|\\.)*)"\s*\)/;

const SEVERITY_RANK: Record<Severity, number> = { unknown: 0, warn: 1, deny: 2 };

function splitClauses(text: string): string[] {
  const segments: string[] = [];
  let depth = 0;
  let start = 0;
  let inString = false;
  for (let i = 0; i < text.length; i += 1) {
    const c = text[i];
    if (inString) {
      if (c === '\\') { i += 1; continue; }
      if (c === '"') inString = false;
      continue;
    }
    if (c === '"') { inString = true; continue; }
    if (c === '(') depth += 1;
    else if (c === ')') depth -= 1;
    else if (c === ';' && depth === 0) {
      segments.push(text.slice(start, i + 1));
      start = i + 1;
    }
  }
  return segments
    .map((s) => s.trim())
    .filter((s) => /\b(forbid|permit)\s*\(/.test(s));
}

function pickSeverity(value: string | undefined): Severity {
  if (value === 'deny' || value === 'warn') return value;
  return 'unknown';
}

function unescape(value: string): string {
  return value.replace(/\\(["\\nrt]|u\{[0-9a-fA-F]+\}|u[0-9a-fA-F]{4})/g, (match, body: string) => {
    if (body === '"') return '"';
    if (body === '\\') return '\\';
    if (body === 'n') return '\n';
    if (body === 'r') return '\r';
    if (body === 't') return '\t';
    if (body.startsWith('u{')) {
      const hex = body.slice(2, -1);
      const cp = parseInt(hex, 16);
      if (cp > 0x10ffff) return match;
      return String.fromCodePoint(cp);
    }
    if (body.startsWith('u')) {
      return String.fromCharCode(parseInt(body.slice(1), 16));
    }
    return match;
  });
}

export function parsePolicyMeta(text: string): PolicyMeta {
  const clauses = splitClauses(text);

  const rules: Rule[] =
    clauses.length === 0
      ? [{ severity: 'unknown', reason: '(no reason annotation)' }]
      : clauses.map((clause) => {
          const sev = clause.match(SEVERITY_RE);
          const reason = clause.match(REASON_RE);
          return {
            severity: pickSeverity(sev?.[1]),
            reason: reason ? unescape(reason[1]) : '(no reason annotation)',
          };
        });

  let dominant: Severity = 'unknown';
  for (const r of rules) {
    if (SEVERITY_RANK[r.severity] > SEVERITY_RANK[dominant]) dominant = r.severity;
  }

  return { rules, dominantSeverity: dominant };
}
