/** HoleSpec.type별 입력 문자열 ↔ HoleValue 직렬화(순수). */
import { i18n } from "../../../i18n";
import type { HoleSpec, HoleValue } from "../../../server-api/policy-store";

export type HoleParse = { ok: true; value: HoleValue } | { ok: false; error: string };

export function parseHoleInput(type: HoleSpec["type"], raw: string): HoleParse {
  const t = raw.trim();
  switch (type) {
    case "long":
    case "decimal": {
      if (!t) return { ok: false, error: i18n.t("editor:holes.numberRequired") };
      const n = Number(t);
      if (!Number.isFinite(n)) return { ok: false, error: i18n.t("editor:holes.numberRequired") };
      return { ok: true, value: type === "long" ? Math.trunc(n) : n };
    }
    case "bool":
      return { ok: true, value: t === "true" };
    case "address":
      return { ok: true, value: t.toLowerCase() };
    case "addressSet":
      return {
        ok: true,
        value: t
          .split(/\n+/)
          .map((s) => s.trim().toLowerCase())
          .filter(Boolean),
      };
    case "string":
      return { ok: true, value: t };
    case "field":
      return t
        ? { ok: true, value: { field: t } }
        : { ok: false, error: i18n.t("editor:holes.fieldPathRequired") };
  }
}

export function formatHoleValue(v: HoleValue | undefined): string {
  if (v === undefined) return "";
  return Array.isArray(v) ? v.map(String).join("\n") : String(v);
}
