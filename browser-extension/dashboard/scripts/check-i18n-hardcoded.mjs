#!/usr/bin/env node
/**
 * Guard against re-introducing hardcoded Korean UI strings.
 *
 * Scans src/**\/*.{ts,tsx} for Hangul outside comments. Korean must live in
 * src/i18n/locales/*.json (or the allowlisted bilingual data files below).
 * Exit 1 with a report when violations are found.
 *
 * Usage: node scripts/check-i18n-hardcoded.mjs
 */

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = join(import.meta.dirname, "..");
const SRC = join(ROOT, "src");

// Bilingual data / grammar files where Korean string literals are by design.
const ALLOWLIST = new Set([
  "src/editor-v9/gloss/paths.ts", // GlossEntry ko/en pairs (translation source)
  "src/editor-v9/manifest-gen/registry.ts", // bilingual label data
  "src/pages/editor/v2/categories.ts", // bilingual CAT table
  "src/cedar/nl.ts", // Korean josa (particle) grammar mechanics
]);

const SKIP_RE = /(__tests__\/|\.test\.tsx?$|\.generated\.ts$|^src\/i18n\/locales\/)/;
const HANGUL = /[가-힣]/;

function* walk(dir) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) yield* walk(p);
    else if (/\.tsx?$/.test(e.name)) yield p;
  }
}

// Naive comment stripping: good enough as a guard, not a parser.
function stripComments(code) {
  return code
    .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "))
    .replace(/(^|[^:'"`])\/\/[^\n]*/g, (m, pre) => pre + " ".repeat(m.length - pre.length));
}

const violations = [];
for (const file of walk(SRC)) {
  const rel = relative(ROOT, file).replaceAll("\\", "/");
  if (SKIP_RE.test(rel) || ALLOWLIST.has(rel)) continue;
  const raw = readFileSync(file, "utf8").split("\n");
  const lines = stripComments(raw.join("\n")).split("\n");
  lines.forEach((line, i) => {
    // `// i18n-ok` on the line opts out (josa grammar args, dev logs, regex ranges)
    if (HANGUL.test(line) && !raw[i].includes("i18n-ok")) {
      violations.push(`${rel}:${i + 1}: ${line.trim()}`);
    }
  });
}

if (violations.length) {
  console.error(`Hardcoded Korean found in ${new Set(violations.map((v) => v.split(":")[0])).size} file(s) — move these into src/i18n/locales/*.json:\n`);
  for (const v of violations) console.error("  " + v);
  process.exit(1);
}
console.log("OK: no hardcoded Korean strings outside i18n resources.");
