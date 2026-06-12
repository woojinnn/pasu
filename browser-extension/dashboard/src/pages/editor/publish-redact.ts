/**
 * Publish-time de-identification.
 *
 * When a curated policy/package goes to the market, address-like literals can
 * leak the publisher's own wallets/allowlists — they are blanked into
 * parameter holes BY DEFAULT (the installer fills their own). Every hole is
 * opt-out per 칸: the author may keep the value public — addresses where the
 * address IS the policy (e.g. "이 주소로 보내면 차단"), numbers as 추천값.
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
// wasm EST→text 렌더러는 getAttr마다 수신자를 괄호로 감싼다 — 중첩 경로는
// `((context.custom).inputUsd)` 모양(실측: est_json_to_policy_text 라운드트립).
// 그래서 경로 패턴은 세그먼트 사이의 괄호를 허용하고, 캡처값은 normPath로
// 점 표기로 정규화해 gloss/폼 leaf의 fieldPath와 맞춘다.
const PATH = `\\(*(?:principal|resource|context|action)(?:\\)*\\.[A-Za-z_]\\w*)+`;
// 토큰 사이의 닫는/여는 괄호 — `(context.x) == "0x…"` 같은 피연산자 래핑.
const CL = `[\\s)]*`; // 토큰 뒤: 공백/닫는 괄호
const OP = `[\\s(]*`; // 토큰 앞: 공백/여는 괄호

/** 렌더된 경로의 괄호를 벗긴 점 표기 — `((context.custom).inputUsd)` → `context.custom.inputUsd`. */
function normPath(raw: string): string {
  return raw.replace(/[()]/g, "");
}

/**
 * 보편 센티널 주소 — 게시자의 개인 값이 아니므로 비식별 대상이 아니다.
 * 제로주소(소각/플레이스홀더), 0x…dead(소각), uint160::MAX(40개의 f —
 * unlimited-approval류의 "무제한" 센티널이 amount 문자열로 비교될 때 주소
 * 모양과 겹친다). 이걸 빼지 않으면 기본 비우기가 센티널을 제로주소로 갈아
 * 끼워 정책 의미를 조용히 바꾼다.
 */
const SENTINEL_ADDR = /^0x(?:0{40}|f{40}|0{36}dead)$/i;

/** raw 리터럴 안의 주소가 전부 센티널이면 비식별 대상이 아니다.
 *  (센티널+개인 주소가 섞인 집합은 개인 값을 가려야 하므로 여전히 대상.) */
function allSentinel(raw: string): boolean {
  const addrs = raw.match(new RegExp(ADDR, "g")) ?? [];
  return addrs.length > 0 && addrs.every((a) => SENTINEL_ADDR.test(a));
}

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
    if (h.kind === "address" && allSentinel(h.raw)) return; // 보편 상수 — 가리지 않는다
    holes.push({ ...h, key: `${ruleId}#${n++}`, ruleId });
  };

  // 1) `[ "0x..", "0x.." ].contains( PATH )` — address set on the left.
  const setContains = new RegExp(
    `(\\[\\s*"${ADDR}"(?:\\s*,\\s*"${ADDR}")*\\s*\\])${CL}\\.contains\\(${OP}(${PATH})`,
    "g",
  );
  for (const m of cedarText.matchAll(setContains)) {
    const raw = m[1];
    const count = (raw.match(new RegExp(ADDR, "g")) ?? []).length;
    const path = normPath(m[2]);
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

  // 1b) `PATH.contains("0x..")` — 폼의 contains/notContains 연산이 내는 모양
  // (집합 필드 ∋ 주소 리터럴). notContains는 `!( … )`로 감싸일 뿐이라 같이 잡힌다.
  const attrContainsAddr = new RegExp(`(${PATH})${CL}\\.contains\\(${OP}("${ADDR}")\\s*\\)`, "g");
  for (const m of cedarText.matchAll(attrContainsAddr)) {
    const path = normPath(m[1]);
    const raw = m[2];
    push({
      kind: "address",
      path,
      label: labelFor(path),
      paramName: paramFor(path),
      display: shortAddr(raw.replace(/"/g, "")),
      raw,
      addrCount: 1,
    });
  }

  // 2) `PATH (== | != | in) "0x.." | [ "0x..", … ]`
  const pathToAddr = new RegExp(
    `(${PATH})${CL}(?:==|!=|in|\\bin\\b)${OP}("${ADDR}"|\\[\\s*"${ADDR}"(?:\\s*,\\s*"${ADDR}")*\\s*\\])`,
    "g",
  );
  for (const m of cedarText.matchAll(pathToAddr)) {
    const path = normPath(m[1]);
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

  // 2b) `"0x.." (== | !=) PATH` — 리터럴이 좌변인 손편집 모양.
  const addrToPath = new RegExp(`("${ADDR}")${CL}(?:==|!=)${OP}(${PATH})`, "g");
  for (const m of cedarText.matchAll(addrToPath)) {
    const raw = m[1];
    const path = normPath(m[2]);
    push({
      kind: "address",
      path,
      label: labelFor(path),
      paramName: paramFor(path),
      display: shortAddr(raw.replace(/"/g, "")),
      raw,
      addrCount: 1,
    });
  }

  // 3) numeric thresholds: `PATH OP decimal("N")` or `PATH OP N`
  const pathToNum = new RegExp(
    `(${PATH})${CL}(>=|<=|>|<|==|!=)${OP}(decimal\\(\\s*"[0-9.]+"\\s*\\)|[0-9]+(?:\\.[0-9]+)?)`,
    "g",
  );
  for (const m of cedarText.matchAll(pathToNum)) {
    const path = normPath(m[1]);
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

  // 3b) decimal 비교는 < <= > >= 가 전부 확장 메소드형 —
  // `PATH.greaterThanOrEqual(decimal("3.0"))` (form convert.ts OP_TO_EXT 참고).
  const pathDecimalMethod = new RegExp(
    `(${PATH})${CL}\\.(?:lessThan|lessThanOrEqual|greaterThan|greaterThanOrEqual)\\(${OP}(decimal\\(\\s*"[0-9.]+"\\s*\\))`,
    "g",
  );
  for (const m of cedarText.matchAll(pathDecimalMethod)) {
    const path = normPath(m[1]);
    const raw = m[2];
    const numMatch = raw.match(/[0-9]+(?:\.[0-9]+)?/);
    push({
      kind: "number",
      path,
      label: labelFor(path),
      paramName: paramFor(path),
      display: numMatch ? numMatch[0] : raw,
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
export const ZERO_ADDR = "0x0000000000000000000000000000000000000000";

/**
 * Apply de-identification to one policy's Cedar text.
 *
 * `keptKeys` — holes the author chose to keep public (주소 공개 / 숫자 추천값
 * 남기기); everything else is replaced with a neutral placeholder.
 */
export function redactCedar(
  cedarText: string,
  holes: PublishHole[],
  keptKeys: ReadonlySet<string>,
): string {
  let out = cedarText;
  for (const h of holes) {
    if (keptKeys.has(h.key)) continue;
    let replacement: string;
    if (h.kind === "address") {
      replacement = h.raw.trim().startsWith("[") ? `["${ZERO_ADDR}"]` : `"${ZERO_ADDR}"`;
    } else if (h.raw.startsWith("decimal(")) {
      // Cedar decimal 리터럴은 소수점 필수 — `decimal("0")`은 설치를 거부당한다.
      replacement = `decimal("0.0")`;
    } else {
      replacement = h.raw.replace(/[0-9]+(?:\.[0-9]+)?/, "0");
    }
    // 같은 리터럴이 정책 안에 여러 번 나와도 전부 치환한다 (extractHoles가
    // raw 기준으로 중복 제거하므로 hole은 하나, 출현은 여럿일 수 있다).
    // 따옴표/괄호가 없는 맨숫자 raw는 다른 토큰의 부분 문자열을 건드리지
    // 않도록 단어 경계로 치환한다.
    if (/^[0-9]/.test(h.raw)) {
      out = out.replace(
        new RegExp(`(?<![\\w.])${h.raw.replace(/\./g, "\\.")}(?![\\w.])`, "g"),
        replacement,
      );
    } else {
      out = out.split(h.raw).join(replacement);
    }
  }
  return out;
}
