/**
 * DEMO (HL_DEMO=1): take REAL open orders from a live Hyperliquid account, rebuild
 * the exact `/exchange` POST wire the dApp would sign, and run them through the
 * ACTUAL extension pipeline — `parseHyperliquidExchangeOrders` (the injected hook
 * parser) → `hlOrderToAction` → `resolveOrderSymbol` (symbol patch) →
 * `collectOrderEnrichment` + `collectHlLeverage` (the 3-call enrichment) — exactly
 * as the service worker does. Prints what comes out for 3 orders.
 *
 * NOT a CI test (opt-in). HL `/info` rate-limits bursts, so this throttles
 * (cool-down + retry-backoff + per-example spacing). The real extension never hits
 * this — it processes one order per user action, not three in a tight loop.
 *
 * Run: HL_DEMO=1 yarn vitest run .../hl-real-order-demo.test.ts
 */
import { writeFileSync } from "node:fs";

import { describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const localStore = new Map<string, unknown>();
  return {
    localStore,
    browser: {
      runtime: { getURL: (p: string) => `chrome-extension://x/${p}` },
      storage: {
        local: {
          get: vi.fn(async (key: string) => ({ [key]: localStore.get(key) })),
          set: vi.fn(async (entries: Record<string, unknown>) => {
            for (const [k, v] of Object.entries(entries)) localStore.set(k, v);
          }),
        },
      },
    },
  };
});
vi.mock("webextension-polyfill", () => ({ default: mocks.browser }));

import { parseHyperliquidExchangeOrders } from "../../../injected/hl-exchange-parse";
import { hlOrderToAction } from "../../hl-order-to-action";
import { resolveOrderSymbol } from "../resolve-order-symbol";
import { collectOrderEnrichment } from "../collect-order-enrichment";
import { collectHlLeverage } from "../collect-hl-leverage";
import { HlInfoClient } from "../hl-info-client";
import type { VenueOrderPayload } from "@lib/types";

const INFO = "https://api.hyperliquid.xyz/info";
const ACCOUNT = "0x010461c14e146ac35fe42271bdc1134ee31c703a";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** A direct `/info` POST that retries on 429 / transient failure (backoff). */
async function postRetry(body: unknown, tries = 6): Promise<any> {
  let delay = 700;
  for (let n = 0; n < tries; n++) {
    try {
      const res = await fetch(INFO, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      if (res.ok) return res.json();
    } catch {
      /* retry */
    }
    await sleep(delay);
    delay = Math.min(delay * 2, 6000);
  }
  throw new Error(`/info ${JSON.stringify(body)} failed after ${tries} tries`);
}

/** Warm the client's (6h-cached) universe so per-order coinForIndex hits cache. */
async function warmUniverse(client: HlInfoClient): Promise<void> {
  for (let n = 0; n < 6; n++) {
    if ((await client.coinForIndex(0)) !== null) return;
    await sleep(1000 * (n + 1));
  }
}

const LIVE = process.env.HL_DEMO === "1";

(LIVE ? describe : describe.skip)("HL real-order demo (3 examples)", () => {
  it("parses 3 real open orders through the live extension pipeline", async () => {
    await sleep(3000); // let any hot rate-limit window cool down

    // 1. Live: coin→index map (meta) + this account's real open orders.
    const meta = await postRetry({ type: "meta" });
    const universe: Array<{ name: string }> = meta.universe;
    const indexOf = (coin: string) => universe.findIndex((u) => u.name === coin);

    const openOrders: any[] = await postRetry({
      type: "frontendOpenOrders",
      user: ACCOUNT,
    });

    // 2. First 3 distinct-coin plain LIMIT orders, for variety.
    const picks: any[] = [];
    const seen = new Set<string>();
    for (const o of openOrders) {
      if (o.isTrigger || seen.has(o.coin)) continue;
      seen.add(o.coin);
      picks.push(o);
      if (picks.length === 3) break;
    }
    expect(picks.length).toBe(3);

    const client = new HlInfoClient({}); // real api.hyperliquid.xyz
    await warmUniverse(client); // ensure symbol resolution can't be starved by a 429

    const lines: string[] = [];
    for (let i = 0; i < picks.length; i++) {
      if (i > 0) await sleep(2500); // space the per-order /info bursts
      const o = picks[i];
      const assetIndex = indexOf(o.coin);
      const isBuy = o.side === "B"; // B = bid/buy, A = ask/sell

      // 3. Rebuild the EXACT `/exchange` order wire the dApp signs: {a,b,p,s,r,t}.
      const wire = {
        a: assetIndex,
        b: isBuy,
        p: String(o.limitPx),
        s: String(o.sz),
        r: o.reduceOnly === true,
        t: { limit: { tif: o.tif } }, // "Alo" | "Gtc" | "Ioc"
      };
      const rawExchangeBody = {
        action: { type: "order", orders: [wire], grouping: "na" },
        nonce: 1_738_000_000_000 + i,
      };

      // 4. EXTENSION PIPELINE — the real modules, in SW order.
      const payloads = parseHyperliquidExchangeOrders(
        "hyperliquid",
        "/exchange",
        "app.hyperliquid.xyz",
        rawExchangeBody,
      );
      const payload = payloads![0] as VenueOrderPayload;
      (payload as any).wallet_id = { address: ACCOUNT, chains: [] }; // fetch-hook stamp

      const { action } = hlOrderToAction(payload);
      const symbolBefore = (action.market as any).symbol;

      const resolved = await resolveOrderSymbol(action, payload, client);
      const symbolAfter = (action.market as any).symbol;

      const [account_leverage, order_enrichment] = await Promise.all([
        collectHlLeverage(action, payload, client),
        collectOrderEnrichment(action, payload, client),
      ]);

      const indent = (v: unknown) =>
        JSON.stringify(v, null, 2)
          .split("\n")
          .map((l) => "    " + l)
          .join("\n");
      lines.push(
        `\n━━━━━━━━ Example ${i + 1}: ${o.coin} ━━━━━━━━\n` +
          `[1] REAL HL open order\n    ${JSON.stringify({ coin: o.coin, side: o.side, limitPx: o.limitPx, sz: o.sz, reduceOnly: o.reduceOnly, tif: o.tif })}\n` +
          `[2] rebuilt /exchange order wire (what the dApp signs)\n    ${JSON.stringify(wire)}\n` +
          `[3] parsed ActionBody (Perp::PlaceOrder)\n${indent(action)}\n` +
          `[4] resolveOrderSymbol:  "${symbolBefore}"  →  "${symbolAfter}"  (resolved=${resolved})\n` +
          `[5] account_leverage (→ context.leverage)\n    ${JSON.stringify(account_leverage)}\n` +
          `[6] order_enrichment (→ context.{notionalUsd, maxLeverage, marginUsedRatioBps, ...})\n${indent(order_enrichment)}`,
      );
      expect(symbolAfter).toBe(o.coin); // the wiring resolved the placeholder
    }

    const report = "\n" + lines.join("\n") + "\n";
    writeFileSync("/tmp/hl_demo_report.txt", report);
    // eslint-disable-next-line no-console
    console.log(report);
  }, 90_000);
});
