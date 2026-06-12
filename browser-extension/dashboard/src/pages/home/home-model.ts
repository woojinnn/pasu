/**
 * Home governance view-model — derives the wallet-dial UI shapes from the ps2
 * StoreSnapshot. Pure functions, no I/O. The components below render these and
 * call the policy-store mutations on toggle.
 *
 *   wallet ─┬─ folder (package)        on/off → setPackageEnabled
 *           │    └─ policy (binding)
 *           │         └─ param (hole)  on/off → updateBinding({ params })
 */
import {
  isEffectiveOn,
  UNCATEGORIZED_PKG,
  type Binding,
  type HoleValue,
  type PolicyDef,
  type StoreSnapshot,
  type WalletPolicyState,
} from "../../server-api/policy-store";

export type Severity = "pass" | "warn" | "fail";

export interface ParamVM {
  holeName: string;
  label: string;
  type: string;
  /** Boolean holes render a live toggle; other holes show their value. */
  isBool: boolean;
  on: boolean;
  /** Display value for non-boolean holes (addressSet/long/decimal/…). */
  display: string;
}

export interface PolicyVM {
  bindingId: string;
  defId: string;
  name: string;
  severity: Severity;
  /** 정책별 o/x — 패키지를 켤 때 이 정책을 포함(o)할지 제외(x)할지 (binding.enabled). */
  enabled: boolean;
  /** 실제 적용 여부 — 패키지 on AND 정책 o. on/off 표시등에 쓴다. */
  effective: boolean;
  params: ParamVM[];
}

export interface FolderVM {
  packageId: string;
  name: string;
  on: boolean;
  policies: PolicyVM[];
}

/** How many baseline (hard-coded, install-time) defs are always applied.
 * Baseline policies are not shown as wallet packages; they count toward the
 * "이 지갑에 적용 N" total. Replace with your real baseline set / flag. */
export const BASELINE_COUNT = 5;

/** Severity is not a first-class field on PolicyDef yet; derive it from the
 * slug the same way the Market does until the server persists it. */
export function severityOf(def: PolicyDef | undefined, defId: string): Severity {
  const id = (def?.id ?? defId).toLowerCase();
  if (/-deny|block|no-|burn|drain/.test(id)) return "fail";
  if (/-warn|confirm|warning|cap|delay/.test(id)) return "warn";
  return "pass";
}

function holeDisplay(v: HoleValue | undefined): string {
  if (v == null) return "—";
  if (Array.isArray(v)) return `${v.length}개`;
  if (typeof v === "object") return `field: ${v.field}`;
  return String(v);
}

function policyVM(snap: StoreSnapshot, b: Binding, packageOn: boolean): PolicyVM {
  const def = snap.library.defs[b.defId];
  const values: Record<string, HoleValue> = { ...(def?.defaults.params ?? {}), ...(b.params ?? {}) };
  const params: ParamVM[] = (def?.holes ?? []).map((h) => {
    const v = values[h.name];
    const isBool = h.type === "bool";
    return {
      holeName: h.name,
      label: h.label || h.name,
      type: h.type,
      isBool,
      on: isBool ? v === true : v != null,
      display: holeDisplay(v),
    };
  });
  return {
    bindingId: b.id,
    defId: b.defId,
    name: b.alias ?? def?.displayName ?? b.defId,
    severity: severityOf(def, b.defId),
    enabled: b.enabled !== false,
    effective: packageOn && b.enabled !== false,
    params,
  };
}

/** Group a wallet's bindings into folders (packages). Empty packages are
 * dropped; the reserved "미분류" package keeps its bindings. */
export function buildFolders(snap: StoreSnapshot, address: string): FolderVM[] {
  const ws: WalletPolicyState | undefined = snap.wallets.byAddress[address.toLowerCase()];
  if (!ws) return [];

  const byPkg = new Map<string, Binding[]>();
  for (const b of Object.values(ws.bindings)) {
    const arr = byPkg.get(b.packageId) ?? [];
    arr.push(b);
    byPkg.set(b.packageId, arr);
  }

  // package display order: wallet packages first, uncategorized last
  const ids = Object.keys(ws.packages).sort((a, b) => {
    if (a === UNCATEGORIZED_PKG) return 1;
    if (b === UNCATEGORIZED_PKG) return -1;
    return (ws.packages[a]?.displayName ?? "").localeCompare(ws.packages[b]?.displayName ?? "", "ko");
  });
  // include any packageId referenced by a binding but missing from packages map
  for (const pid of byPkg.keys()) if (!ids.includes(pid)) ids.push(pid);

  return ids
    .map((pid): FolderVM => {
      const bindings = (byPkg.get(pid) ?? []).sort((a, b) => {
        // 제외(x = enabled false)된 정책은 항상 최하단으로.
        if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;
        const an = snap.library.defs[a.defId]?.displayName ?? a.defId;
        const bn = snap.library.defs[b.defId]?.displayName ?? b.defId;
        return an.localeCompare(bn, "ko");
      });
      const on = ws.packageEnabled[pid] ?? true;
      return {
        packageId: pid,
        name: ws.packages[pid]?.displayName ?? (pid === UNCATEGORIZED_PKG ? "미분류" : pid),
        on,
        policies: bindings.map((b) => policyVM(snap, b, on)),
      };
    })
    .filter((f) => f.policies.length > 0);
}

/** Policy count across all packages (for the wallet-card "정책 N" badge). */
export function totalPolicyCount(snap: StoreSnapshot, address: string): number {
  const ws = snap.wallets.byAddress[address.toLowerCase()];
  return ws ? Object.keys(ws.bindings).length : 0;
}

/** 이 지갑에 적용 = baseline + effective bindings (package ∧ binding on). */
export function appliedCount(snap: StoreSnapshot, address: string): number {
  const ws = snap.wallets.byAddress[address.toLowerCase()];
  if (!ws) return BASELINE_COUNT;
  let n = 0;
  for (const b of Object.values(ws.bindings)) if (isEffectiveOn(ws, b)) n++;
  return BASELINE_COUNT + n;
}

/** Next params object for a boolean hole toggle. */
export function toggledParams(
  current: Record<string, HoleValue> | undefined,
  holeName: string,
  on: boolean,
): Record<string, HoleValue> {
  return { ...(current ?? {}), [holeName]: on };
}
