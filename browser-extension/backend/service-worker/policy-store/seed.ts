/** builtin 시드 + 구(v1) 키 정리.
 *
 *  day1-safety baked 정책(`default-policies/policy-set-v2.json`)을 wasm
 *  text→EST→BlockIR 역변환으로 builtin 정의 + "기본 안전팩" 패키지로 흡수한다.
 *  baked의 "항상 평가" 특례는 resolve의 미등록-지갑 default 경로가 대체한다. */
import Browser from "webextension-polyfill";

import { estToBlocks } from "../../../sdk/block-ir/estToBlocks";
import type { EstPolicy } from "../../../sdk/block-ir/est";
import { policyTextToEst } from "../wasm-bridge";
import { mutate, readStore } from "./store";
import type { PolicyDef } from "./types";

export const BUILTIN_PKG = "pkg::builtin.day1-safety";

/** 계정당 한 번만 시드 (SW 수명 내 캐시 + storage의 builtin def 존재로 멱등). */
const seededUids = new Set<string>();

export function clearSeedCache(): void {
  seededUids.clear();
}

export async function ensureSeeded(uid: string): Promise<void> {
  if (seededUids.has(uid)) return;
  const s = await readStore(uid);
  if (Object.values(s.library.defs).some((d) => d.source === "builtin")) {
    seededUids.add(uid);
    return;
  }

  const res = await fetch(Browser.runtime.getURL("default-policies/policy-set-v2.json"));
  if (!res.ok) throw new Error(`baked set fetch failed: HTTP ${res.status}`);
  const baked = JSON.parse(await res.text()) as { id: string; policy: string; manifest?: unknown }[];

  const defs: PolicyDef[] = [];
  for (const b of baked) {
    try {
      const parsed = JSON.parse(await policyTextToEst(b.policy)) as {
        ok: boolean;
        policies?: { id: string; est: unknown }[];
      };
      if (!parsed.ok || !parsed.policies?.[0]) throw new Error("text→EST 변환 실패");
      const ir = estToBlocks(parsed.policies[0].est as EstPolicy, null);
      defs.push({
        id: `def::builtin.${b.id.replace(/[^A-Za-z0-9_.-]/g, "-")}`,
        displayName: b.id,
        skeleton: { ir, manifest: b.manifest },
        holes: [],
        defaults: { enabled: true, params: {}, packageId: BUILTIN_PKG },
        source: "builtin",
        updatedAtMs: Date.now(),
      });
    } catch (err) {
      // 손상 항목은 건너뜀 (best-effort) — 나머지 builtin 보호는 유지
      console.warn(`[Dambi] builtin 정책 시드 실패 — 건너뜀: ${b.id}`, err);
    }
  }

  await mutate(uid, (d) => {
    d.library.packages[BUILTIN_PKG] = {
      id: BUILTIN_PKG,
      displayName: "기본 안전팩",
      source: "builtin",
      updatedAtMs: Date.now(),
    };
    for (const def of defs) d.library.defs[def.id] = def;
  });
  seededUids.add(uid);
}

const LEGACY_PREFIXES = ["dashboard:policies:", "dashboard:sets:", "policy-selection:", "migration:"];

/** v1 정책 스토리지 네임스페이스 제거 — 마이그레이션 없는 리셋(스펙 합의). */
export async function cleanupLegacyKeys(): Promise<void> {
  const all = (await Browser.storage.local.get(null)) as Record<string, unknown>;
  const doomed = Object.keys(all).filter((k) => LEGACY_PREFIXES.some((p) => k.startsWith(p)));
  if (doomed.length > 0) await Browser.storage.local.remove(doomed);
}
