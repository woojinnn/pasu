/**
 * Market-scoped locale, now a thin wrapper over the app-wide i18next language
 * (src/i18n). Market copy lives in the `market` i18n namespace; this hook
 * remains only for locale-parameterized helpers (pickI18n, categoryNameOf,
 * publisherDisplay, install-v2) that pick between server-provided ko/en data.
 */

import { useTranslation } from "react-i18next";

export type MarketLocale = "ko" | "en";

/**
 * Read the active locale and a setter. Backed by i18next — setting persists
 * via the i18n languageChanged listener and re-renders all subscribers.
 */
export function useMarketLocale(): [MarketLocale, (next: MarketLocale) => void] {
  const { i18n } = useTranslation();
  const locale: MarketLocale = i18n.language === "en" ? "en" : "ko";
  const setLocale = (next: MarketLocale) => {
    void i18n.changeLanguage(next);
  };
  return [locale, setLocale];
}
