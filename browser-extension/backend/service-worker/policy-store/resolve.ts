/** 평가용 번들 해석: tx.from → 지갑의 effective 바인딩 → 렌더된 {policy, manifest}.
 *
 *  미등록 지갑은 defaults.enabled 정의를 기본 파라미터로 받는다(안전 우선 —
 *  baked "항상 평가" 특례의 대체물). 액션 사전 필터는 최적화일 뿐이며 정밀
 *  게이트는 엔진(WASM trigger 매칭)이 수행한다 — 따라서 여기서는 "확실히 매칭
 *  불가"일 때만 드롭하고, 모르면 통과시킨다. */
import { ensureSeeded } from "./seed";
import { readStore } from "./store";
import { renderDef } from "./render";
import { isEffectiveOn, type HoleValue, type PolicyDef } from "./types";

export interface ResolvedBundle {
  id: string;
  policy: string;
  manifest: unknown;
  /** manifest trigger.where에서 파생한 사전 필터 인덱스. 없으면 항상 포함. */
  trigger?: { tags?: string[]; domains?: string[]; venues?: string[] } | undefined;
}

/** manifest trigger.where의 eq/in 제약만 인덱스로 추출. ne/nin·tx.* 필드는
 *  보수적으로 무시(그 차원은 필터하지 않음 = 항상 포함 쪽으로). */
export function extractTrigger(manifest: unknown): ResolvedBundle["trigger"] {
  const where = (manifest as { trigger?: { where?: Record<string, unknown> } } | undefined)?.trigger?.where;
  if (!where || typeof where !== "object") return undefined;
  const pick = (k: string): string[] | undefined => {
    const c = where[k] as { eq?: unknown; in?: unknown } | undefined;
    if (!c || typeof c !== "object") return undefined;
    if (typeof c.eq === "string") return [c.eq];
    if (Array.isArray(c.in) && c.in.every((v) => typeof v === "string")) return c.in as string[];
    return undefined;
  };
  const tags = pick("action.tag");
  const domains = pick("action.domain");
  const venues = pick("action.venue");
  if (!tags && !domains && !venues) return undefined;
  const out: NonNullable<ResolvedBundle["trigger"]> = {};
  if (tags) out.tags = tags;
  if (domains) out.domains = domains;
  if (venues) out.venues = venues;
  return out;
}

/** 디코딩된 ActionBody(JSON)의 트리거-관련 필드.
 *  - string: 알려진 값
 *  - null: 확실한 부재 — 엔진에서 eq/in이 미스하는 포지션(예: multicall 부모의 tag)
 *  - undefined: 읽기 실패/모름 → 그 차원은 필터하지 않는다(엔진이 판정) */
export interface ActionMeta {
  domain?: string | null | undefined;
  tag?: string | null | undefined;
  venue?: string | null | undefined;
}

/** 액션 body(JSON, multicall 포함)에서 자기 자신 + 모든 leaf의 메타를 수집.
 *  multicall은 자식 포지션에서 번들이 발화할 수 있으므로 leaf 합집합으로
 *  필터해야 한다. */
export function collectActionMetas(body: unknown): ActionMeta[] {
  if (typeof body !== "object" || body === null) return [{}];
  const b = body as { domain?: unknown; action?: unknown; venue?: unknown; actions?: unknown };
  if (typeof b.domain !== "string") return [{}]; // 디코딩 안 된 body — 전부 unknown
  const venueRaw = b.venue as { name?: unknown } | string | undefined;
  // 디코딩된 body에서 필드 부재는 엔진의 None과 같다(확실한 부재 = null).
  const self: ActionMeta = {
    domain: b.domain,
    tag: typeof b.action === "string" ? b.action : null,
    venue:
      typeof venueRaw === "string"
        ? venueRaw
        : venueRaw && typeof venueRaw === "object"
          ? typeof venueRaw.name === "string"
            ? venueRaw.name
            : undefined // venue는 있는데 형태를 모름 → unknown
          : null,
  };
  const metas = [self];
  if (b.domain === "multicall" && Array.isArray(b.actions)) {
    for (const child of b.actions) metas.push(...collectActionMetas(child));
  }
  return metas;
}

/** 어떤 메타도 트리거를 만족할 수 없을 때만 드롭. unknown(undefined) 차원은
 *  통과, 확실 부재(null)는 엔진과 동일하게 eq/in 미스로 취급. */
export function filterForAction(bundles: ResolvedBundle[], metas: ActionMeta[]): ResolvedBundle[] {
  const dimOk = (allow: string[] | undefined, v: string | null | undefined) =>
    !allow || v === undefined || (v !== null && allow.includes(v));
  return bundles.filter(
    (b) =>
      !b.trigger ||
      metas.some(
        (m) => dimOk(b.trigger!.tags, m.tag) && dimOk(b.trigger!.domains, m.domain) && dimOk(b.trigger!.venues, m.venue),
      ),
  );
}

export async function resolveBundlesForWallet(uid: string, fromAddress: string): Promise<ResolvedBundle[]> {
  await ensureSeeded(uid);
  const s = await readStore(uid);
  const addr = fromAddress.toLowerCase();
  const w = s.wallets.byAddress[addr];

  const wanted: { defId: string; params: Record<string, HoleValue> }[] = [];
  // 정의 수정으로 사라진 홀의 잔존 파라미터 키는 렌더 실패(unknown param)를
  // 일으키므로 현재 holes 목록으로 거른다.
  const knownParams = (def: { holes: { name: string }[] }, merged: Record<string, HoleValue>) => {
    const live = new Set(def.holes.map((h) => h.name));
    return Object.fromEntries(Object.entries(merged).filter(([k]) => live.has(k)));
  };
  if (w) {
    for (const b of Object.values(w.bindings)) {
      if (!isEffectiveOn(w, b)) continue;
      const def = s.library.defs[b.defId];
      if (!def) continue; // validate가 막지만 방어적으로
      wanted.push({ defId: b.defId, params: knownParams(def, { ...def.defaults.params, ...b.params }) });
    }
  } else {
    // 미등록 지갑: defaults.enabled 정의를 기본 파라미터로 (안전 우선)
    for (const def of Object.values(s.library.defs)) {
      if (def.defaults.enabled) {
        wanted.push({ defId: def.id, params: knownParams(def, def.defaults.params) });
      }
    }
  }

  const out: ResolvedBundle[] = [];
  for (const { defId, params } of wanted) {
    const def = s.library.defs[defId];
    try {
      const r = await renderDef(def, params);
      // 보강 필드가 없는 정책은 manifest가 없다 — 엔진의 plan/evaluate 입력은
      // ManifestV2 구조체가 필수라(null이 섞이면 invalid_input_json으로 그 지갑의
      // 평가 전체가 죽는다) 빈 manifest(트리거 없음=항상 평가, 호출 없음)를 합성한다.
      const manifest = r.manifest ?? emptyManifestFor(def);
      out.push({ id: defId, policy: r.text, manifest, trigger: extractTrigger(manifest) });
    } catch (err) {
      // 한 정의의 손상이 전체 평가를 막지 않게 — 그 정의만 건너뛴다.
      console.warn(`[Pasu] 정책 렌더 실패 — 건너뜀: ${defId}`, err);
    }
  }
  return out;
}

/** manifest 없는 def의 평가용 최소 ManifestV2. trigger/policy_rpc/custom_context는
 *  엔진 쪽 default(빈 값)로 채워진다. id는 Cedar `@id`(verdict의 policy_id와
 *  일치) 우선, 없으면 def.id. */
function emptyManifestFor(def: PolicyDef): { id: string; schema_version: number } {
  const ann = (def.skeleton.ir as { annotations?: { name: string; value: string }[] } | null)
    ?.annotations;
  const cedarId = Array.isArray(ann) ? ann.find((a) => a.name === "id")?.value : undefined;
  return { id: cedarId ?? def.id, schema_version: 2 };
}

/** verdict의 matched policy_id(=Cedar @id annotation) → def 참조.
 *  verdict 기록 시점에 def_id/display_name을 박제하는 용도(P3) — 이후 이름
 *  변경/삭제와 무관하게 과거 기록이 자립한다. ① IR @id annotation ② def.id. */
export async function defRefForPolicyId(
  uid: string,
  policyId: string,
): Promise<{ defId: string; displayName: string } | null> {
  const s = await readStore(uid);
  for (const d of Object.values(s.library.defs)) {
    const ann = (d.skeleton.ir as { annotations?: { name: string; value: string }[] } | null)
      ?.annotations;
    if (Array.isArray(ann) && ann.some((a) => a.name === "id" && a.value === policyId)) {
      return { defId: d.id, displayName: d.displayName };
    }
  }
  const direct = s.library.defs[policyId];
  return direct ? { defId: direct.id, displayName: direct.displayName } : null;
}
