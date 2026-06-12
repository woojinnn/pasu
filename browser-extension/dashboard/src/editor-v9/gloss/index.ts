import { i18n } from "../../i18n";
import {
  ROLE_LABEL_KO,
  ROLE_LABEL_EN,
  type GlossEntry,
  type Role,
} from "./paths";

export {
  allGloss,
  getGloss,
  glossByRole,
  blockTypeForPath,
  pathForBlockType,
  ROLE_COLOUR,
  ROLE_LABEL_KO,
  ROLE_LABEL_EN,
  type FieldKind,
  type GlossEntry,
  type Role,
} from "./paths";

/** True when the active i18n language is English (gloss data is ko/en only). */
function isEn(): boolean {
  return i18n.language?.startsWith("en") ?? false;
}

/** Locale-aware display label for a gloss entry (follows `i18n.language`). */
export function glossLabel(entry: GlossEntry): string {
  return isEn() ? entry.en : entry.ko;
}

/** Locale-aware one-line description for a gloss entry. */
export function glossDesc(entry: GlossEntry): string {
  return isEn() ? entry.desc.en : entry.desc.ko;
}

/** Locale-aware unit suffix for a gloss entry (undefined when unit-less). */
export function glossUnit(entry: GlossEntry): string | undefined {
  return entry.unit ? (isEn() ? entry.unit.en : entry.unit.ko) : undefined;
}

/** Locale-aware toolbox category label for a gloss role. */
export function roleLabel(role: Role): string {
  return isEn() ? ROLE_LABEL_EN[role] : ROLE_LABEL_KO[role];
}
