/**
 * App-wide i18n (i18next + react-i18next). Resources are statically bundled
 * (no http backend — extension pages must work under CSP and offline).
 *
 * Init is a function, NOT a module-level side effect: it must run AFTER
 * migratePasuRenameLocalStorage() in main.tsx, and static imports would hoist
 * an auto-init above that call.
 */

import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import koCommon from "./locales/ko/common.json";
import koShell from "./locales/ko/shell.json";
import koHome from "./locales/ko/home.json";
import koEditor from "./locales/ko/editor.json";
import koBlocks from "./locales/ko/blocks.json";
import koMarket from "./locales/ko/market.json";
import koMonitoring from "./locales/ko/monitoring.json";
import koHistory from "./locales/ko/history.json";
import koSimulation from "./locales/ko/simulation.json";
import koFields from "./locales/ko/fields.json";
import koActions from "./locales/ko/actions.json";
import koDiagnosis from "./locales/ko/diagnosis.json";

import enCommon from "./locales/en/common.json";
import enShell from "./locales/en/shell.json";
import enHome from "./locales/en/home.json";
import enEditor from "./locales/en/editor.json";
import enBlocks from "./locales/en/blocks.json";
import enMarket from "./locales/en/market.json";
import enMonitoring from "./locales/en/monitoring.json";
import enHistory from "./locales/en/history.json";
import enSimulation from "./locales/en/simulation.json";
import enFields from "./locales/en/fields.json";
import enActions from "./locales/en/actions.json";
import enDiagnosis from "./locales/en/diagnosis.json";

export type AppLocale = "ko" | "en";

export const NAMESPACES = [
  "common",
  "shell",
  "home",
  "editor",
  "blocks",
  "market",
  "monitoring",
  "history",
  "simulation",
  "fields",
  "actions",
  "diagnosis",
] as const;

const LOCALE_KEY = "pasu:locale";
/** Pre-i18n market-only locale toggle; used once as a seed, never written. */
const LEGACY_MARKET_LOCALE_KEY = "pasu:market-locale";

function readInitialLocale(): AppLocale {
  // try/catch: under Node >=22 the experimental WebStorage global can shadow
  // jsdom's localStorage in tests and throw on access.
  try {
    const v =
      window.localStorage.getItem(LOCALE_KEY) ??
      window.localStorage.getItem(LEGACY_MARKET_LOCALE_KEY);
    return v === "en" ? "en" : "ko";
  } catch {
    return "ko";
  }
}

export function initI18n(): typeof i18n {
  if (i18n.isInitialized) return i18n;

  void i18n.use(initReactI18next).init({
    lng: readInitialLocale(),
    fallbackLng: "ko",
    defaultNS: "common",
    ns: [...NAMESPACES],
    resources: {
      ko: {
        common: koCommon,
        shell: koShell,
        home: koHome,
        editor: koEditor,
        blocks: koBlocks,
        market: koMarket,
        monitoring: koMonitoring,
        history: koHistory,
        simulation: koSimulation,
        fields: koFields,
        actions: koActions,
        diagnosis: koDiagnosis,
      },
      en: {
        common: enCommon,
        shell: enShell,
        home: enHome,
        editor: enEditor,
        blocks: enBlocks,
        market: enMarket,
        monitoring: enMonitoring,
        history: enHistory,
        simulation: enSimulation,
        fields: enFields,
        actions: enActions,
        diagnosis: enDiagnosis,
      },
    },
    interpolation: { escapeValue: false }, // React escapes already
    returnEmptyString: false,
  });

  i18n.on("languageChanged", (lng) => {
    try {
      window.localStorage.setItem(LOCALE_KEY, lng);
    } catch {
      // non-browser / broken storage: language still switches, just not persisted
    }
  });

  return i18n;
}

export { i18n };
