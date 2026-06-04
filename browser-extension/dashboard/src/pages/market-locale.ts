/**
 * Market-scoped locale state. Persisted to localStorage so the choice
 * sticks across reloads. Kept as a tiny module (no Context) — the market
 * pages read/write directly via the hook.
 */

import { useEffect, useState } from "react";

export type MarketLocale = "ko" | "en";

const STORAGE_KEY = "scopeball:market-locale";

function read(): MarketLocale {
  if (typeof window === "undefined") return "ko";
  const v = window.localStorage.getItem(STORAGE_KEY);
  return v === "en" ? "en" : "ko";
}

/**
 * Read the active market locale and a setter. Setting persists to localStorage
 * and broadcasts a custom event so sibling pages re-render.
 */
export function useMarketLocale(): [MarketLocale, (next: MarketLocale) => void] {
  const [locale, setLocaleState] = useState<MarketLocale>(read);

  useEffect(() => {
    const onChange = () => setLocaleState(read());
    window.addEventListener("scopeball:locale-change", onChange);
    return () => window.removeEventListener("scopeball:locale-change", onChange);
  }, []);

  const setLocale = (next: MarketLocale) => {
    window.localStorage.setItem(STORAGE_KEY, next);
    window.dispatchEvent(new Event("scopeball:locale-change"));
    setLocaleState(next);
  };

  return [locale, setLocale];
}
