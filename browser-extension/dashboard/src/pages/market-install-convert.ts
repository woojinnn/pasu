/** 마켓 리스팅 → ps2 def[] 변환(순수, 변환기 주입). 서버/브리지 의존 없음. */
import type { PolicyDef } from "../../../sdk/policy-store-types";
import type { PolicyIR } from "../cedar/blocks";

export interface ListingMeta {
  id: string;
  kind: "policy" | "set";
  displayName: string;
  version: string;
  cat: string | undefined;
}

export interface VersionBody {
  cedar_text?: string;
  manifest?: unknown;
  members?: { slug: string; cedar_text: string; manifest?: unknown; display_name?: string }[];
}

/** 변환 실패 항목이 있으면 전체 설치 중단(부분 설치 없음). */
export async function listingToDefs(
  meta: ListingMeta,
  body: VersionBody,
  toBlocks: (t: string) => Promise<PolicyIR[]>,
): Promise<PolicyDef[]> {
  const items =
    meta.kind === "set"
      ? (body.members ?? []).map((m) => ({
          id: `def::market.${meta.id}.${m.slug}`,
          name: m.display_name || m.slug,
          cedar: m.cedar_text,
          manifest: m.manifest,
        }))
      : [
          {
            id: `def::market.${meta.id}`,
            name: meta.displayName,
            cedar: body.cedar_text ?? "",
            manifest: body.manifest,
          },
        ];
  if (items.length === 0) throw new Error("리스팅에 설치할 정책이 없어요");

  const defs: PolicyDef[] = [];
  for (const it of items) {
    let ir: PolicyIR | undefined;
    try {
      ir = (await toBlocks(it.cedar))[0];
    } catch {
      ir = undefined;
    }
    if (!ir) throw new Error(`정책 "${it.name}"을(를) 설치 형식으로 변환할 수 없어요`);
    defs.push({
      id: it.id,
      displayName: it.name,
      cat: meta.cat,
      skeleton: { ir, manifest: it.manifest },
      holes: [],
      defaults: { enabled: false, params: {}, packageId: undefined }, // 설치 선택이 채움
      source: "market",
      sourceListingId: meta.id,
      sourceVersion: meta.version,
      updatedAtMs: Date.now(),
    });
  }
  return defs;
}
