/** 렌더 파이프라인: def 뼈대(BlockIR)+파라미터 → Cedar text + hole-치환 manifest.
 *  캐시 키 = (defId, updatedAtMs, params) — 뼈대 수정이나 파라미터 변경 시 미스. */
import type { PolicyIR } from "../../../sdk/block-ir/ir";
import { blocksToEst } from "../../../sdk/block-ir/blocksToEst";
import { fillParams } from "../../../sdk/block-ir/params";
import { estToPolicyText } from "../wasm-bridge";
import type { HoleValue, PolicyDef } from "./types";

export interface RenderedPolicy {
  text: string;
  manifest: unknown;
}

const MAX_CACHE = 256;
const cache = new Map<string, RenderedPolicy>();

/** Wire schema version of an empty fallback manifest. Mirrors the Rust
 *  `MANIFEST_V2_SCHEMA_VERSION` (policy_rpc/manifest_v2.rs); ManifestV2 requires
 *  only `{id, schema_version}` — the rest default. */
const MANIFEST_V2_SCHEMA_VERSION = 2;

export function clearRenderCache(): void {
  cache.clear();
}

/** manifest 안의 `{"$hole": "<name>"}` 단독-키 객체를 파라미터 값으로 깊은-치환. */
export function substituteHoles(node: unknown, params: Record<string, HoleValue>): unknown {
  if (Array.isArray(node)) return node.map((n) => substituteHoles(n, params));
  if (node && typeof node === "object") {
    const o = node as Record<string, unknown>;
    if (typeof o.$hole === "string" && Object.keys(o).length === 1) {
      if (!(o.$hole in params)) throw new Error(`manifest hole 미충족: ${o.$hole}`);
      return params[o.$hole];
    }
    return Object.fromEntries(Object.entries(o).map(([k, v]) => [k, substituteHoles(v, params)]));
  }
  return node;
}

export async function renderDef(def: PolicyDef, params: Record<string, HoleValue>): Promise<RenderedPolicy> {
  const sortedKeys = Object.keys(params).sort();
  const key = `${def.id}|${def.updatedAtMs}|${JSON.stringify(params, sortedKeys)}`;
  const hit = cache.get(key);
  if (hit) return hit;

  const filled = fillParams(def.skeleton.ir as PolicyIR, params);
  if (!filled.ok) {
    throw new Error(`파라미터 오류 (${def.id}): ${filled.errors.map((e) => `${e.name}: ${e.message}`).join(", ")}`);
  }
  const est = blocksToEst(filled.policy);
  const raw = JSON.parse(await estToPolicyText(JSON.stringify(est))) as {
    ok: boolean;
    text?: string;
    error?: string;
  };
  if (!raw.ok || !raw.text) throw new Error(`EST→Cedar 실패 (${def.id}): ${raw.error ?? "?"}`);
  // A def may legitimately carry NO manifest (an HL/perp policy whose enrichment
  // is SW-direct, or a marketplace listing published without one). The rendered
  // manifest must still be a VALID ManifestV2 — never `undefined`/`null` — because
  // the evaluator serializes `bundles.map(b => b.manifest)` and the WASM
  // `ManifestV2` deserialize rejects a null element, which would kill the whole
  // evaluation. An empty manifest has no trigger/policy_rpc, so the policy is
  // still cedar-evaluated (any enrichment arrives SW-direct).
  const rawManifest =
    def.skeleton.manifest === undefined ? undefined : substituteHoles(def.skeleton.manifest, params);
  const manifest = rawManifest ?? { id: def.id, schema_version: MANIFEST_V2_SCHEMA_VERSION };

  const out: RenderedPolicy = { text: raw.text, manifest };
  if (cache.size >= MAX_CACHE) cache.delete(cache.keys().next().value as string);
  cache.set(key, out);
  return out;
}
