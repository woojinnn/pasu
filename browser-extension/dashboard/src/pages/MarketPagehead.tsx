/**
 * Market-only in-content page header — a 1:1 port of the prototype's
 * `.rm-pagehead` (js/market-final.js `shell()`), NOT the global `<Topbar>`.
 *
 * The prototype draws this sticky bar *inside* the content column, and only on
 * the list / detail views (`shell(crumb, back, …)`); the landing view calls
 * `shell("", null, …)` so it has no header at all — just the body's big
 * `.rm-shead-ttl "Market"`. Reproducing that means: the landing renders NO
 * header, list/detail render this `<MarketPagehead>`.
 *
 *   ┌ rm-pagehead (sticky, 60px, blur) ─────────────────────────┐
 *   │ [Market]  /  <crumb>                         [← back] │
 *   └────────────────────────────────────────────────────────────┘
 *
 * `.logo "Market"` links home (`/market`); `back` is the right-aligned return
 * link. Both are real router links here (the prototype used in-SPA state).
 */
import { useEffect } from "react";
import { Link } from "react-router-dom";

export function MarketPagehead({
  crumb,
  back,
}: {
  /** Path segment shown after "Market /". Omit on the bare home view. */
  crumb?: string;
  /** Right-aligned back link: where to go + its label. */
  back?: { to: string; label: string };
}) {
  return (
    <div className="rm-pagehead">
      <Link to="/market" className="logo">
        Market
      </Link>
      {crumb && (
        <>
          <span className="rm-sep">/</span>
          <span className="crumb">{crumb}</span>
        </>
      )}
      {back && (
        <Link to={back.to} className="rm-back">
          {back.label}
        </Link>
      )}
    </div>
  );
}

/**
 * Toggles a `market-route` marker class on the shared `.app-content` element
 * for the lifetime of a market page. The prototype kills the shell padding
 * (`.app-content { padding: 0 }`) so `.rm-page` is the sole frame; we scope
 * that to the market route only, leaving every other page's padding intact.
 *
 * Call once at the top of each market page component.
 */
export function useMarketContentClass(): void {
  useEffect(() => {
    const el = document.querySelector(".app-content");
    el?.classList.add("market-route");
    return () => el?.classList.remove("market-route");
  }, []);
}
