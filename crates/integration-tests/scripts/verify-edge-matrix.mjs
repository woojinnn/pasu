#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const repoRoot = path.resolve(path.dirname(__filename), "../../..");

const args = new Map();
for (let i = 2; i < process.argv.length; i += 1) {
  const arg = process.argv[i];
  if (!arg.startsWith("--")) continue;
  const next = process.argv[i + 1];
  if (next && !next.startsWith("--")) {
    args.set(arg, next);
    i += 1;
  } else {
    args.set(arg, "true");
  }
}

const protocol = args.get("--protocol") ?? "uniswap";
const matrixPath = path.resolve(
  repoRoot,
  args.get("--matrix") ??
    `crates/integration-tests/onboarding/${protocol}/edge-matrix.json`,
);

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function rel(file) {
  return path.relative(repoRoot, file);
}

function corpusPath(name) {
  return path.resolve(repoRoot, `crates/integration-tests/data/golden/v3-decode/${name}/corpus.json`);
}

function loadCorpus(name, cache) {
  if (!cache.has(name)) {
    const file = corpusPath(name);
    if (!fs.existsSync(file)) {
      throw new Error(`corpus ${name} missing at ${rel(file)}`);
    }
    const parsed = readJson(file);
    if (!Array.isArray(parsed.transactions)) {
      throw new Error(`corpus ${name} has no transactions[]`);
    }
    cache.set(name, parsed.transactions);
  }
  return cache.get(name);
}

function validateCorpusRef(row, artifact, cache) {
  const transactions = loadCorpus(artifact.protocol ?? protocol, cache);
  const tx = transactions.find((candidate) => {
    if (artifact.intent && candidate.intent !== artifact.intent) return false;
    if (artifact.tx_hash && String(candidate.tx_hash ?? "").toLowerCase() !== artifact.tx_hash.toLowerCase()) {
      return false;
    }
    return true;
  });
  if (!tx) {
    throw new Error(`${row.id}: corpus ref not found: ${JSON.stringify(artifact)}`);
  }
  if (row.expect && tx.expect !== row.expect) {
    throw new Error(`${row.id}: expected corpus verdict ${row.expect}, got ${tx.expect}`);
  }
  if (tx.expect === "pass" && !Array.isArray(tx.expect_body)) {
    throw new Error(`${row.id}: pass corpus row lacks expect_body`);
  }
  if (tx.expect === "pass" && tx.expect_body.length === 0) {
    throw new Error(`${row.id}: pass corpus row has empty expect_body`);
  }
  if (tx.expect === "error" && typeof tx.expect_error !== "string") {
    throw new Error(`${row.id}: error corpus row lacks expect_error`);
  }
}

function validateTestRef(row, artifact) {
  const file = path.resolve(repoRoot, artifact.file ?? "");
  if (!fs.existsSync(file)) {
    throw new Error(`${row.id}: test file missing: ${artifact.file}`);
  }
  const body = fs.readFileSync(file, "utf8");
  for (const symbol of artifact.symbols ?? []) {
    if (!body.includes(symbol)) {
      throw new Error(`${row.id}: test symbol ${symbol} missing in ${artifact.file}`);
    }
  }
}

const matrix = readJson(matrixPath);
const required = new Set(matrix.required_categories ?? []);
const covered = new Set();
const cache = new Map();
const rows = matrix.cases ?? [];

if (!rows.length) {
  throw new Error(`${rel(matrixPath)} has no cases[]`);
}

for (const row of rows) {
  if (!row.id) throw new Error("edge matrix row missing id");
  if (!Array.isArray(row.categories) || row.categories.length === 0) {
    throw new Error(`${row.id}: categories[] required`);
  }
  if (!row.artifact || typeof row.artifact !== "object") {
    throw new Error(`${row.id}: artifact required`);
  }
  if (row.disposition === "covered") {
    for (const category of row.categories) covered.add(category);
  }
  if (row.artifact.kind === "corpus") {
    validateCorpusRef(row, row.artifact, cache);
  } else if (row.artifact.kind === "test") {
    validateTestRef(row, row.artifact);
  } else {
    throw new Error(`${row.id}: unsupported artifact kind ${row.artifact.kind}`);
  }
}

const missing = [...required].filter((category) => !covered.has(category));
if (missing.length > 0) {
  throw new Error(`required edge categories missing covered cases: ${missing.join(", ")}`);
}

console.log(
  JSON.stringify(
    {
      matrix: rel(matrixPath),
      protocol,
      cases: rows.length,
      required_categories: [...required],
      covered_categories: [...covered].sort(),
    },
    null,
    2,
  ),
);
