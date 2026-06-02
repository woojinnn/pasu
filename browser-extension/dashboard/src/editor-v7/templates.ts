/**
 * Built-in policy templates, bundled with the dashboard.
 *
 * Server-side `policy-templates.json` was retired when policy storage
 * moved to chrome.storage.local. The seven templates that used to live
 * there now ship as a static JSON import alongside the v7 editor so
 * `getPolicyTemplates()` resolves synchronously without a network roundtrip.
 *
 * Add a template by editing `templates.json`. Each entry needs:
 *   - id        : stable slug
 *   - name      : { ko, en } i18n
 *   - description: { ko, en }
 *   - severity  : "deny" | "warn" | "info"
 *   - cedar_text: full cedar policy text (with `@id` / `@severity` /
 *                 `@reason` annotations and type-narrowed permit/forbid)
 */

import templatesJson from "./templates.json";

export interface PolicyTemplate {
  id: string;
  name: { ko: string; en: string };
  description: { ko: string; en: string };
  severity: "deny" | "warn" | "info";
  cedar_text: string;
}

const TEMPLATES = templatesJson as readonly PolicyTemplate[];

export function listTemplates(): readonly PolicyTemplate[] {
  return TEMPLATES;
}

export function getTemplate(id: string): PolicyTemplate | undefined {
  return TEMPLATES.find((t) => t.id === id);
}
