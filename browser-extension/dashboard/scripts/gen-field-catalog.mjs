/**
 * Codegen: parse the canonical Cedar schema (schema/policy-schema/*.cedarschema)
 * into a per-action field catalog the form/block pickers consume.
 *
 * For each action it resolves `context: <Type>` through the type registry
 * (Core::TokenRef, venue records, nested SwapDirection, inline records …) and
 * emits every COMPARABLE leaf path with its Cedar type and the exact `has`
 * presence-guards needed (one per optional step in the chain).
 *
 * Output is a compact tuple form to keep the bundle small:
 *   [path, cedarType]                       — required leaf
 *   [path, cedarType, [[of,attr],…]]        — optional leaf + its has-guards
 *
 * Run:  node scripts/gen-field-catalog.mjs            (writes the .generated.ts)
 *       node scripts/gen-field-catalog.mjs --inspect  (prints a few actions)
 *
 * i18n note: the generated curated-field-meta.generated.ts is the Korean
 * source of truth for field labels/descs. The English translations live in
 * src/i18n/locales/en/fields-curated.json as a manually maintained overlay.
 * When regenerating with new fields, add matching en entries there.
 */
import { readFileSync, readdirSync, statSync, writeFileSync } from "fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";

const HERE = dirname(fileURLToPath(import.meta.url));
const SCHEMA_DIR = join(HERE, "../../../schema/policy-schema");
const OUT = join(HERE, "../src/cedar/form/schema-catalog.generated.ts");

const PRIM = new Set(["String", "Long", "Bool", "decimal"]);

function stripComments(s) {
  return s.replace(/\/\/[^\n]*/g, "").replace(/\/\*[\s\S]*?\*\//g, "");
}
function splitTop(inner, sep) {
  const out = [];
  let depth = 0, buf = "";
  for (const c of inner) {
    if ("({[<".includes(c)) depth++;
    else if (")}]>".includes(c)) depth--;
    if (c === sep && depth === 0) { out.push(buf); buf = ""; }
    else buf += c;
  }
  if (buf.trim()) out.push(buf);
  return out;
}
function matchBrace(s, open) {
  let depth = 0;
  for (let i = open; i < s.length; i++) {
    if (s[i] === "{") depth++;
    else if (s[i] === "}") { depth--; if (depth === 0) return i; }
  }
  return -1;
}
function parseType(expr, curNs) {
  const s = expr.trim();
  if (s.startsWith("Set<")) return { kind: "set" };
  if (s.startsWith("{")) {
    const close = matchBrace(s, s.indexOf("{"));
    return { kind: "record", fields: parseFields(s.slice(s.indexOf("{") + 1, close), curNs) };
  }
  if (PRIM.has(s)) return { kind: "prim", name: s };
  if (s.includes("::")) { const [ns, name] = s.split("::"); return { kind: "name", ns, name }; }
  return { kind: "name", ns: curNs, name: s };
}
function parseFields(body, curNs) {
  const fields = [];
  for (const raw of splitTop(stripComments(body), ",")) {
    const seg = raw.trim();
    if (!seg) continue;
    const m = seg.match(/^([A-Za-z_]\w*)\s*(\??)\s*:\s*([\s\S]+)$/);
    if (!m) continue;
    fields.push({ name: m[1], optional: m[2] === "?", type: parseType(m[3], curNs) });
  }
  return fields;
}
function walk(d) {
  let o = [];
  for (const e of readdirSync(d)) {
    const p = join(d, e);
    if (statSync(p).isDirectory()) o = o.concat(walk(p));
    else if (p.endsWith(".cedarschema")) o.push(p);
  }
  return o;
}

const types = {};
const actions = [];
function addType(ns, name, fields) { (types[ns] ??= {})[name] = { fields }; }

for (const file of walk(SCHEMA_DIR)) {
  const src = stripComments(readFileSync(file, "utf8"));
  const nsRe = /namespace\s+([A-Za-z_]\w*)\s*\{/g;
  let m;
  const spans = [];
  while ((m = nsRe.exec(src))) {
    const open = src.indexOf("{", m.index);
    const close = matchBrace(src, open);
    spans.push({ ns: m[1], body: src.slice(open + 1, close) });
  }
  for (const { ns, body } of spans) {
    const tRe = /type\s+([A-Za-z_]\w*)\s*=\s*/g;
    let tm;
    while ((tm = tRe.exec(body))) {
      const after = body.slice(tRe.lastIndex).replace(/^\s*/, "");
      if (after.startsWith("{")) {
        const open = body.indexOf("{", tRe.lastIndex);
        const close = matchBrace(body, open);
        addType(ns, tm[1], parseFields(body.slice(open + 1, close), ns));
        tRe.lastIndex = close + 1;
      } else {
        const semi = body.indexOf(";", tRe.lastIndex);
        addType(ns, tm[1], [{ name: "__alias__", optional: false, type: parseType(body.slice(tRe.lastIndex, semi), ns) }]);
        tRe.lastIndex = semi + 1;
      }
    }
    const aRe = /action\s+"([^"]+)"\s+appliesTo\s*\{/g;
    let am;
    while ((am = aRe.exec(body))) {
      const open = body.indexOf("{", am.index);
      const close = matchBrace(body, open);
      const inner = body.slice(open + 1, close);
      const cm = inner.match(/context\s*:\s*([A-Za-z_]\w*(?:::[A-Za-z_]\w*)?)/);
      if (cm) actions.push({ ns, id: am[1], ctxType: cm[1] });
    }
  }
}

function resolve(typeRef) {
  if (typeRef.kind === "prim" || typeRef.kind === "set" || typeRef.kind === "record") return typeRef;
  const rec = types[typeRef.ns]?.[typeRef.name];
  if (!rec) return { kind: "unknown" };
  if (rec.fields.length === 1 && rec.fields[0].name === "__alias__") return resolve(rec.fields[0].type);
  return { kind: "record", fields: rec.fields };
}
function expand(typeRef, path, guards, out, depth) {
  if (depth > 7) return;
  const r = resolve(typeRef);
  if (r.kind === "prim") return out.push({ path, t: r.name, guards });
  if (r.kind === "set") return out.push({ path, t: "Set", guards });
  if (r.kind === "unknown") return;
  for (const f of r.fields) {
    const childPath = `${path}.${f.name}`;
    const childGuards = f.optional ? [...guards, [path, f.name]] : guards;
    expand(f.type, childPath, childGuards, out, depth + 1);
  }
}

// Drop pure-plumbing leaves the picker never offers (keeps the bundle small;
// the form can't pick them, so no has-guards are needed for them):
//   - the common envelope `context.meta.*`
//   - TokenKey discriminators `*.key.{standard,contract,tokenId}` (keep address)
function isPlumbing(path) {
  if (path.startsWith("context.meta")) return true;
  return /\.key\.(standard|contract|tokenId)$/.test(path);
}

const catalog = {};
for (const a of actions) {
  const out = [];
  expand(parseType(a.ctxType, a.ns), "context", [], out, 0);
  catalog[`${a.ns}::${a.id}`] = out
    .filter((f) => !isPlumbing(f.path))
    .map((f) => (f.guards.length ? [f.path, f.t, f.guards] : [f.path, f.t]));
}

// Synthetic "*" = any-action: union of every distinct path, fully guarded
// (nothing is guaranteed present when no action is scoped). First type wins.
const anyMap = new Map();
for (const fields of Object.values(catalog)) {
  for (const [p, t] of fields) if (!anyMap.has(p)) anyMap.set(p, t);
}
function fullGuards(path) {
  // guard every step from context down: context has a, context.a has b, …
  const segs = path.split(".");
  const g = [];
  for (let i = 1; i < segs.length; i++) g.push([segs.slice(0, i).join("."), segs[i]]);
  return g;
}
catalog["*"] = [...anyMap.entries()].map(([p, t]) => [p, t, fullGuards(p)]);

if (process.argv.includes("--inspect")) {
  for (const key of ["Amm::Swap", "Token::Erc20Transfer", "Lending::Borrow", "Perp::ChangeLeverage", "Governance::Delegate", "*"]) {
    console.log(`\n### ${key} (${catalog[key]?.length ?? 0} leaves)`);
    for (const f of (catalog[key] ?? []).slice(0, 40))
      console.log(`  ${f[0]} :${f[1]}${f[2] ? "  [" + f[2].map((g) => g[0] + " has " + g[1]).join("; ") + "]" : ""}`);
  }
  process.exit(0);
}

const body = Object.entries(catalog)
  .map(([k, v]) => `  ${JSON.stringify(k)}: ${JSON.stringify(v)},`)
  .join("\n");

// Every declared action as [namespace, id] (sorted) — drives the form's
// "무엇을 검사하나요?" picker so it offers every action the schema knows.
const actionList = actions
  .map((a) => [a.ns, a.id])
  .sort((x, y) => (x[0] + x[1]).localeCompare(y[0] + y[1]));

const out = `// AUTO-GENERATED by scripts/gen-field-catalog.mjs — DO NOT EDIT.
// Source of truth: schema/policy-schema/*.cedarschema. Regenerate:
//   node scripts/gen-field-catalog.mjs
//
// Compact tuple form per leaf:
//   [path, cedarType]                  — required (no guard)
//   [path, cedarType, [[of,attr],…]]   — optional; each pair is a \`<of> has <attr>\`
// Key \`*\` is the any-action union (every path fully presence-guarded).

export type RawGuard = [string, string];
export type RawField = [string, string] | [string, string, RawGuard[]];

export const SCHEMA_CATALOG: Record<string, RawField[]> = {
${body}
};

/** Every schema action as \`[namespace, actionId]\`. The Cedar action entity
 *  type is \`\${namespace}::Action\`. */
export const SCHEMA_ACTIONS: ReadonlyArray<readonly [string, string]> = ${JSON.stringify(actionList)};
`;
writeFileSync(OUT, out);
console.log(`wrote ${OUT}: ${Object.keys(catalog).length} keys, ${actionList.length} actions`);
