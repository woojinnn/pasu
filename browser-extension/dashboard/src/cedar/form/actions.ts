/**
 * The "무엇을 검사하나요?" (trigger) action list, derived from the schema so the
 * picker offers EVERY action the engine knows — not a hand-curated subset.
 *
 * Source of truth: {@link SCHEMA_ACTIONS} (codegen from the .cedarschema files).
 * This module only adds localized labels + domain grouping on top. Labels live
 * in the i18n "actions" namespace (`group.<ns>` / `name.<ns>.<id>`) and are
 * resolved at CALL time via lazy getters, so the exported `KNOWN_ACTIONS` /
 * `ACTION_GROUPS` arrays keep their original const shape while always speaking
 * the current language.
 */

import { i18n } from "../../i18n";

import { SCHEMA_ACTIONS } from "./schema-catalog.generated";

/** A selectable action for the trigger dropdown. `entityType`/`id` map to the
 *  Cedar scope `action == entityType::"id"`. */
export interface KnownAction {
  entityType: string;
  id: string;
  label: string;
  /** Localized domain group, for `<optgroup>` rendering. */
  group: string;
}

/** A stable display order for the domain groups (group labels live in the
 *  "actions" i18n namespace under `group.<ns>`). */
const GROUP_ORDER = [
  "Token", "Amm", "Perp", "HyperliquidCore", "Lending", "Yield",
  "Staking", "LiquidStaking", "Restaking", "Governance", "Airdrop",
  "Launchpad", "Marketplace", "Permission", "Core",
];

/** Localized group label for a namespace; the namespace itself if untranslated. */
function groupLabel(ns: string): string {
  const key = `group.${ns}`;
  return i18n.exists(key, { ns: "actions" }) ? i18n.t(key, { ns: "actions" }) : ns;
}

/** Localized label for the common actions; the long tail humanises its id. */
function actionLabel(ns: string, id: string): string {
  const key = `name.${ns}.${id}`;
  return i18n.exists(key, { ns: "actions" }) ? i18n.t(key, { ns: "actions" }) : humanize(id);
}

/** Split a PascalCase id into spaced words as a last-resort label. */
function humanize(id: string): string {
  return id.replace(/([a-z0-9])([A-Z])/g, "$1 $2");
}

/** Collation locale matching the language the labels are rendered in. */
function sortLocale(): string {
  return i18n.language === "en" ? "en" : "ko";
}

/** Namespaces hidden from the trigger picker. */
const EXCLUDED_NS = new Set(["Yield"]);
const PICKABLE_ACTIONS = SCHEMA_ACTIONS.filter(([ns]) => !EXCLUDED_NS.has(ns));

/** Every schema action, labelled + grouped. `label`/`group` are getters that
 *  resolve through i18n at access time (call-time localization). */
export const KNOWN_ACTIONS: KnownAction[] = PICKABLE_ACTIONS.map(([ns, id]) => ({
  entityType: `${ns}::Action`,
  id,
  get label() {
    return actionLabel(ns, id);
  },
  get group() {
    return groupLabel(ns);
  },
}));

/** Actions bucketed by domain group, in display order, for `<optgroup>`.
 *  `group` and the per-group `actions` ordering resolve at access time so the
 *  list re-sorts under the active locale. */
export const ACTION_GROUPS: { group: string; actions: KnownAction[] }[] = (() => {
  const byNs = new Map<string, KnownAction[]>();
  for (const [ns] of PICKABLE_ACTIONS) if (!byNs.has(ns)) byNs.set(ns, []);
  for (const a of KNOWN_ACTIONS) {
    const ns = a.entityType.split("::")[0];
    byNs.get(ns)!.push(a);
  }
  const order = [...GROUP_ORDER, ...[...byNs.keys()].filter((n) => !GROUP_ORDER.includes(n))];
  return order
    .filter((ns) => byNs.has(ns))
    .map((ns) => ({
      get group() {
        return groupLabel(ns);
      },
      get actions() {
        const locale = sortLocale();
        return byNs.get(ns)!.slice().sort((a, b) => a.label.localeCompare(b.label, locale));
      },
    }));
})();
