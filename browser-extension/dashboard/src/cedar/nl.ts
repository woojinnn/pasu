/**
 * Plain-Korean phrasing for a single condition, shared by the form's row chips
 * and the policy diagram's leaf labels. The caller supplies the already-rendered
 * subject (field label) and value text; this only handles the operator phrasing
 * and Korean particles (이/가, 을/를).
 */

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

/** Render a condition as a Korean phrase, e.g. "수신자가 비어 있을 때". */
export function naturalCondition({ subject, op, value, emptyStr, neg }: NLCondition): string {
  const S = withJosa(subject, "이", "가");
  const V = value;
  switch (op) {
    case "==":
      if (emptyStr) return `${S} ${neg ? "비어 있지 않을" : "비어 있을"} 때`;
      return neg ? `${S} ${withJosa(V, "이", "가")} 아닐 때` : `${S} ${V}일 때`;
    case "!=":
      if (emptyStr) return `${S} ${neg ? "비어 있을" : "비어 있지 않을"} 때`;
      return neg ? `${S} ${V}일 때` : `${S} ${withJosa(V, "이", "가")} 아닐 때`;
    case "<":
      return `${S} ${V}보다 ${neg ? "크거나 같을" : "작을"} 때`;
    case "<=":
      return `${S} ${V} ${neg ? "초과일" : "이하일"} 때`;
    case ">":
      return `${S} ${V}보다 ${neg ? "작거나 같을" : "클"} 때`;
    case ">=":
      return `${S} ${V} ${neg ? "미만일" : "이상일"} 때`;
    case "in":
      return `${S} ${V} 중 ${neg ? "하나도 아닐" : "하나일"} 때`;
    case "contains":
      return `${S} ${withJosa(V, "을", "를")} ${neg ? "포함하지 않을" : "포함할"} 때`;
    default:
      return neg ? `${subject} ${op} ${V} 아님` : `${subject} ${op} ${V}`;
  }
}
