/**
 * Publish-time de-identification.
 *
 * When a curated policy/package goes to the market, address-like literals must
 * not leak the publisher's own wallets/allowlists — they are ALWAYS blanked
 * into parameter holes (the installer fills their own). Numeric thresholds are
 * optional: the author may keep a recommended value or blank it.
 *
 * We work over the raw Cedar text with focused regexes (no async IR round-trip)
 * so the publish wizard can preview holes synchronously. Redaction replaces the
 * matched literal with a neutral placeholder so the published Cedar stays valid
 * and carries no real values.
 */

import { allGloss, getGloss } from "../../editor-v9/gloss/paths";

export type HoleKind = "address" | "number";

export interface PublishHole {
  /** Stable key (ruleId + index) for React + decision lookup. */
  key: string;
  ruleId: string;
  kind: HoleKind;
  /** Cedar attribute path the literal is compared against, e.g. context.recipient. */
  path: string;
  /** Human label (gloss) for the field. */
  label: string;
  /** Parameter-hole name shown to the user, e.g. ?wallet. */
  paramName: string;
  /** Pretty original value for display (0xAbc…1234, "150", 3 addresses…). */
  display: string;
  /** Exact substring in the source Cedar to replace on redaction. */
  raw: string;
  /** Unit suffix for numbers (bp / USD / 초). */
  unit?: string;
  /** For address sets: how many addresses were collapsed. */
  addrCount?: number;
}

const ADDR = `0x[0-9a-fA-F]{40}`;
const PATH = `(?:principal|resource|context|action)(?:\\.[A-Za-z_]\\w*)+`;

/** Friendly fallbacks for paths the gloss doesn't carry. */
const PATH_LABEL: Record<string, string> = {
  "principal.address": "내 지갑 주소",
  "resource.allowlist": "허용 목록(allowlist)",
  "resource.address": "대상 주소",
};

/** Map a Cedar path to a parameter-hole name. */
function paramFor(path: string): string {
  if (path === "principal.address") return "?wallet";
  const seg = path.split(".").pop() ?? "value";
  return `?${seg}`;
}

function labelFor(path: string): string {
  return getGloss(path)?.ko ?? PATH_LABEL[path] ?? path;
}

function unitFor(path: string): string | undefined {
  return getGloss(path)?.unit?.ko;
}

function ruleIdOf(cedarText: string): string {
  const m = cedarText.match(/@id\(\s*"([^"]+)"\s*\)/);
  return m ? m[1] : "policy";
}

function shortAddr(a: string): string {
  return `${a.slice(0, 6)}…${a.slice(-4)}`;
}

/**
 * Extract every blank-able literal (addresses + numeric thresholds) from one
 * policy's Cedar text.
 */
export function extractHoles(cedarText: string): PublishHole[] {
  const ruleId = ruleIdOf(cedarText);
  const holes: PublishHole[] = [];
  const seen = new Set<string>(); // raw substrings already captured
  let n = 0;

  const push = (h: Omit<PublishHole, "key" | "ruleId">) => {
    if (seen.has(h.raw)) return;
    seen.add(h.raw);
    holes.push({ ...h, key: `${ruleId}#${n++}`, ruleId });
  };

  // 1) `[ "0x..", "0x.." ].contains( PATH )` — address set on the left.
  const setContains = new RegExp(
    `(\\[\\s*"${ADDR}"(?:\\s*,\\s*"${ADDR}")*\\s*\\])\\s*\\.contains\\(\\s*(${PATH})`,
    "g",
  );
  for (const m of cedarText.matchAll(setContains)) {
    const raw = m[1];
    const count = (raw.match(new RegExp(ADDR, "g")) ?? []).length;
    const path = m[2];
    push({
      kind: "address",
      path,
      label: labelFor(path),
      paramName: paramFor(path),
      display: `주소 ${count}개`,
      raw,
      addrCount: count,
    });
  }

  // 2) `PATH (== | != | in) "0x.." | [ "0x..", … ]`
  const pathToAddr = new RegExp(
    `(${PATH})\\s*(?:==|!=|in|\\bin\\b)\\s*("${ADDR}"|\\[\\s*"${ADDR}"(?:\\s*,\\s*"${ADDR}")*\\s*\\])`,
    "g",
  );
  for (const m of cedarText.matchAll(pathToAddr)) {
    const path = m[1];
    const raw = m[2];
    const count = (raw.match(new RegExp(ADDR, "g")) ?? []).length;
    push({
      kind: "address",
      path,
      label: labelFor(path),
      paramName: paramFor(path),
      display: count > 1 ? `주소 ${count}개` : shortAddr(raw.replace(/"/g, "")),
      raw,
      addrCount: count,
    });
  }

  // 3) numeric thresholds: `PATH OP decimal("N")` or `PATH OP N`
  const pathToNum = new RegExp(
    `(${PATH})\\s*(>=|<=|>|<|==|!=)\\s*(decimal\\(\\s*"[0-9.]+"\\s*\\)|[0-9]+(?:\\.[0-9]+)?)`,
    "g",
  );
  for (const m of cedarText.matchAll(pathToNum)) {
    const path = m[1];
    const raw = m[3];
    const numMatch = raw.match(/[0-9]+(?:\.[0-9]+)?/);
    const num = numMatch ? numMatch[0] : raw;
    push({
      kind: "number",
      path,
      label: labelFor(path),
      paramName: paramFor(path),
      display: num,
      raw,
      unit: unitFor(path),
    });
  }

  return holes;
}

/** A runtime address-role field the policy references (e.g. principal.address).
 *  It carries no literal in the text — it's already per-user/parametric — but
 *  we surface it so the publisher sees every identifier the policy touches is
 *  blanked. Informational: redaction does not change the Cedar. */
export interface AddressFieldRef {
  path: string;
  label: string;
  paramName: string;
}

/** Address-role field paths the policy references but that are NOT literals
 *  (those are already covered by {@link extractHoles}). */
export function addressFieldRefs(
  cedarText: string,
  literalPaths: ReadonlySet<string>,
): AddressFieldRef[] {
  const out: AddressFieldRef[] = [];
  const seen = new Set<string>();
  for (const g of allGloss()) {
    if (g.role !== "address") continue;
    if (literalPaths.has(g.path) || seen.has(g.path)) continue;
    // word-ish boundary so context.recipient doesn't match context.recipients
    const re = new RegExp(g.path.replace(/[.]/g, "\\.") + "(?![\\w.])");
    if (re.test(cedarText)) {
      seen.add(g.path);
      out.push({ path: g.path, label: g.ko, paramName: paramFor(g.path) });
    }
  }
  // principal.address is not in the gloss table but is the canonical "my wallet"
  // reference; surface it explicitly.
  if (
    !seen.has("principal.address") &&
    !literalPaths.has("principal.address") &&
    /principal\.address(?![\w.])/.test(cedarText)
  ) {
    out.push({
      path: "principal.address",
      label: PATH_LABEL["principal.address"],
      paramName: "?wallet",
    });
  }
  return out;
}

/** Address placeholder the installer overwrites. Zero address = "fill me". */
const ZERO_ADDR = "0x0000000000000000000000000000000000000000";

/**
 * Apply de-identification to one policy's Cedar text.
 *
 * `keptNumberKeys` — number holes the author chose to keep (추천값 남기기);
 * everything else (all addresses, and blanked numbers) is replaced with a
 * neutral placeholder.
 */
export function redactCedar(
  cedarText: string,
  holes: PublishHole[],
  keptNumberKeys: ReadonlySet<string>,
): string {
  let out = cedarText;
  for (const h of holes) {
    if (h.kind === "number" && keptNumberKeys.has(h.key)) continue;
    const replacement =
      h.kind === "address"
        ? h.raw.trim().startsWith("[")
          ? `[${ZERO_ADDR ? `"${ZERO_ADDR}"` : ""}]`
          : `"${ZERO_ADDR}"`
        : h.raw.replace(/[0-9]+(?:\.[0-9]+)?/, "0");
    out = out.replace(h.raw, replacement);
  }
  return out;
}
