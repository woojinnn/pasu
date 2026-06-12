/**
 * Plain-language phrasing for a single condition, shared by the form's row
 * chips and the policy diagram's leaf labels. The caller supplies the
 * already-rendered subject (field label) and value text; this only handles the
 * operator phrasing. Templates live in the i18n "fields" namespace (`nl.*`)
 * and are resolved at CALL time; Korean particles (이/가, 을/를) are computed
 * here and interpolated, so the ko output is identical to the original
 * hand-written phrasing.
 */

import { i18n } from "../i18n";

/** Whether a word ends in a Korean final consonant (받침). Falls back for
 *  digit/latin endings (last digit's Korean reading for numerals). */
export function hasJong(word: string): boolean {
  const ch = word.at(-1);
  if (!ch) return false;
  const code = ch.charCodeAt(0);
  if (code >= 0xac00 && code <= 0xd7a3) return (code - 0xac00) % 28 !== 0;
  if (ch >= "0" && ch <= "9") return [false, true, false, true, false, false, true, true, true, false][Number(ch)];
  return false;
}

/** Attach the right particle to `word` (jong = with 받침, no = without). */
export function withJosa(word: string, jong: string, no: string): string {
  return word + (hasJong(word) ? jong : no);
}

export interface NLCondition {
  /** Field label, e.g. "수신자". */
  subject: string;
  /** Operator: `==` `!=` `<` `<=` `>` `>=` `contains` `in`. */
  op: string;
  /** Rendered value, e.g. `"0xabc…"`, `10`, `내 지갑 주소`, `[a, b]`. */
  value: string;
  /** The value is an empty string literal (→ "비어 있을 때"). */
  emptyStr?: boolean;
  /** Negate the condition. */
  neg?: boolean;
}

/** `nl.*` template key for an operator (+ its negation / empty-string form). */
function nlKey(op: string, neg: boolean, emptyStr: boolean): string | null {
  switch (op) {
    case "==":
      if (emptyStr) return neg ? "notEmpty" : "empty";
      return neg ? "eqNeg" : "eq";
    case "!=":
      if (emptyStr) return neg ? "empty" : "notEmpty";
      return neg ? "eq" : "eqNeg";
    case "<":
      return neg ? "ltNeg" : "lt";
    case "<=":
      return neg ? "leNeg" : "le";
    case ">":
      return neg ? "gtNeg" : "gt";
    case ">=":
      return neg ? "geNeg" : "ge";
    case "in":
      return neg ? "inNeg" : "in";
    case "contains":
      return neg ? "containsNeg" : "contains";
    default:
      return null;
  }
}

/** Render a condition as a natural phrase, e.g. "수신자가 비어 있을 때"
 *  (en: "when recipient is empty"). */
export function naturalCondition({ subject, op, value, emptyStr, neg }: NLCondition): string {
  const ko = i18n.language !== "en";
  const V = value;
  const vars = {
    // Subject with its topic particle attached (ko only): "수신자" → "수신자가".
    S: ko ? withJosa(subject, "이", "가") : subject,
    V,
    // Value with 이/가 (the `… 아닐 때` phrasing) / 을/를 (the `… 포함할 때`
    // phrasing) — identity in en.
    Vj: ko ? withJosa(V, "이", "가") : V,
    Vc: ko ? withJosa(V, "을", "를") : V,
    subject,
    op,
    value,
  };
  const key = nlKey(op, !!neg, !!emptyStr);
  if (key) return i18n.t(`nl.${key}`, { ns: "fields", ...vars });
  return i18n.t(neg ? "nl.fallbackNeg" : "nl.fallback", { ns: "fields", ...vars });
}
