/**
 * Dashboard feature flags.
 *
 * Each flag gates a single user-visible change so we can ship the
 * mypolicy redesign one screen at a time. Default is off; flip to
 * `true` (in-code) once the corresponding phase ships and bakes.
 *
 * Dev override: set `VITE_FEATURES=newListView,newChooser` in your
 * env to force flags on at build time. Unrecognised names are
 * silently ignored.
 */

type FeatureName =
  | "newListView"
  | "newChooser"
  | "newEditorView"
  | "marketUpdateBadge";

const DEFAULTS: Record<FeatureName, boolean> = {
  newListView: true,
  newChooser: true,
  newEditorView: true,
  marketUpdateBadge: true,
};

function parseEnvOverrides(): Partial<Record<FeatureName, boolean>> {
  const raw = (import.meta.env?.VITE_FEATURES as string | undefined) ?? "";
  if (!raw) return {};
  const out: Partial<Record<FeatureName, boolean>> = {};
  for (const name of raw.split(",").map((s) => s.trim()).filter(Boolean)) {
    if (name in DEFAULTS) {
      out[name as FeatureName] = true;
    }
  }
  return out;
}

export const FEATURES: Record<FeatureName, boolean> = {
  ...DEFAULTS,
  ...parseEnvOverrides(),
};
