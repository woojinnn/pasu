/** popup의 패키지 카드 파생(순수) — ps2 라이브러리 + 활성 지갑 상태 →
 *  표시용 패키지/멤버 구조. store.js(plain JS)가 window.PasuPs2로 사용한다. */

interface LibDoc {
  defs: Record<
    string,
    {
      id: string;
      displayName: string;
      source?: string;
      skeleton?: { ir?: unknown };
    }
  >;
  packages: Record<string, { id: string; displayName: string }>;
}

interface WalletState {
  bindings: Record<
    string,
    { id: string; defId: string; packageId: string; enabled: boolean; alias?: string }
  >;
  /** 지갑 소속 패키지 — 이름의 1차 출처(라이브러리 폴더와 별개). */
  packages?: Record<string, { id: string; displayName: string }>;
  packageEnabled: Record<string, boolean>;
}

export interface PopupPkgMember {
  bindingId: string;
  defId: string;
  name: string;
  sev: string;
  enabled: boolean;
}

export interface PopupPkg {
  id: string;
  name: string;
  on: boolean;
  members: PopupPkgMember[];
}

function sevOf(ir: unknown): string {
  const ann = (ir as { annotations?: { name: string; value: string }[] } | null)?.annotations;
  if (Array.isArray(ann)) {
    const v = ann.find((a) => a.name === "severity")?.value;
    if (v === "deny" || v === "warn" || v === "info") return v;
  }
  return "warn";
}

export function derivePopupPackages(lib: LibDoc, wallet: WalletState | null): PopupPkg[] {
  const w = wallet ?? { bindings: {}, packageEnabled: {} };
  const byPkg = new Map<string, PopupPkgMember[]>();
  for (const b of Object.values(w.bindings)) {
    const def = lib.defs[b.defId];
    const arr = byPkg.get(b.packageId) ?? [];
    arr.push({
      bindingId: b.id,
      defId: b.defId,
      name: b.alias ?? def?.displayName ?? b.defId,
      sev: sevOf(def?.skeleton?.ir),
      enabled: b.enabled,
    });
    byPkg.set(b.packageId, arr);
  }
  const nameOf = (pid: string) =>
    pid === "pkg::uncategorized"
      ? "미분류"
      : (w.packages?.[pid]?.displayName ?? lib.packages[pid]?.displayName ?? pid);
  return [...byPkg.keys()]
    .sort((a, b) =>
      a.startsWith("pkg::builtin") ? -1 : b.startsWith("pkg::builtin") ? 1 : a.localeCompare(b),
    )
    .map((pid) => ({
      id: pid,
      name: nameOf(pid),
      on: w.packageEnabled[pid] ?? true,
      members: (byPkg.get(pid) ?? []).sort((a, b) => a.name.localeCompare(b.name, "ko")),
    }));
}

/** 온보딩 step3용 builtin 베이스라인 목록. */
export function deriveBaseline(lib: LibDoc): { id: string; title: string; sev: string }[] {
  return Object.values(lib.defs)
    .filter((d) => d.source === "builtin")
    .map((d) => ({ id: d.id, title: d.displayName, sev: sevOf(d.skeleton?.ir) }))
    .sort((a, b) => a.title.localeCompare(b.title, "ko"));
}
