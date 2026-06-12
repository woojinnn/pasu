#!/usr/bin/env node
/**
 * Automated browser proof that the Dambi extension BLOCKS a Hyperliquid
 * `/exchange` short order by policy, while letting a long order through.
 *
 * Drives a real Chrome (the built extension loaded) over the DevTools protocol
 * and asserts the MAIN-world fetch hook's verdict beacon:
 *   - SHORT (b=false) → window.__dambi_last_verdict__.allowed === false   (blocked)
 *   - LONG  (b=true)  → window.__dambi_last_verdict__.allowed === true    (allowed)
 *
 * Prereqs:
 *   1. yarn build:chrome          # builds dist/chrome with the deny seed policy
 *   2. python3 -m http.server 8099 --directory scripts/demo   # serve the page
 *   3. Launch Chrome (HEADED — see note) with the extension + debug port:
 *        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
 *          --user-data-dir=/tmp/hl-demo-profile --no-first-run \
 *          --disable-extensions-except="$PWD/dist/chrome" \
 *          --load-extension="$PWD/dist/chrome" \
 *          --remote-debugging-port=9222 \
 *          --window-position=-2400,-2400 about:blank &
 *   4. node scripts/demo/hl-deny-browser-demo.mjs
 *
 * NOTE: use HEADED Chrome (an offscreen window position works for CI). Chrome
 * `--headless=new` does NOT inject `world: "MAIN"` content scripts, so the fetch
 * hook never installs there — the demo would see no block. This is a headless
 * limitation, not a bug in the hook (verified: headed installs, headless does not).
 */
import WebSocket from "ws";

const DEBUG_PORT = process.env.CDP_PORT ?? "9222";
const PAGE_URL = process.env.DEMO_URL ?? "http://localhost:8099/hl-deny-demo-page.html";

function connect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  let id = 0;
  const pending = new Map();
  ws.on("message", (raw) => {
    const m = JSON.parse(raw.toString());
    if (m.id && pending.has(m.id)) {
      const { resolve, reject } = pending.get(m.id);
      pending.delete(m.id);
      m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result);
    }
  });
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const i = ++id;
      pending.set(i, { resolve, reject });
      ws.send(JSON.stringify({ id: i, method, params }));
    });
  return { ws, send, open: new Promise((r) => ws.on("open", r)) };
}

const orderExpr = (isBuy) => `(async () => {
  const body = { action: { type:"order", orders:[
    { a:0, b:${isBuy}, p:"60000", s:"0.1", r:false, t:{limit:{tif:"Gtc"}} }
  ], grouping:"na" }, nonce: Date.now() };
  let outcome;
  try {
    const r = await fetch("https://api.hyperliquid.xyz/exchange",
      { method:"POST", headers:{"content-type":"application/json"}, body: JSON.stringify(body) });
    outcome = { ok:true, status:r.status };
  } catch (e) { outcome = { ok:false, error: e && e.message }; }
  return { outcome, verdict: window.__dambi_last_verdict__ };
})()`;

async function main() {
  const version = await (
    await fetch(`http://localhost:${DEBUG_PORT}/json/version`)
  ).json();
  const browser = connect(version.webSocketDebuggerUrl);
  await browser.open;

  const { targetId } = await browser.send("Target.createTarget", { url: PAGE_URL });
  await new Promise((r) => setTimeout(r, 1200));
  const targets = await (await fetch(`http://localhost:${DEBUG_PORT}/json`)).json();
  const pageTarget = targets.find((t) => t.id === targetId);
  const page = connect(pageTarget.webSocketDebuggerUrl);
  await page.open;
  await page.send("Runtime.enable");
  await page.send("Page.enable");

  // Reload once: the FIRST page load right after `--load-extension` typically
  // races extension readiness and misses content-script injection (a Chrome
  // quirk, not specific to MAIN world). A reload injects deterministically.
  await page.send("Page.reload", { ignoreCache: true });
  await new Promise((r) => setTimeout(r, 3000));

  const evalExpr = async (expr) => {
    const r = await page.send("Runtime.evaluate", {
      expression: expr,
      awaitPromise: true,
      returnByValue: true,
    });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.text);
    return r.result.value;
  };

  const installed = await evalExpr(
    "!!window[Symbol.for('__dambi_fetch_hook_install_state__')]",
  );
  if (!installed) {
    console.error(
      "FAIL: fetch hook not installed in MAIN world. Use HEADED Chrome (see header note).",
    );
    process.exit(2);
  }

  const short = await evalExpr(orderExpr(false));
  const long = await evalExpr(orderExpr(true));

  console.log("SHORT:", JSON.stringify(short));
  console.log("LONG :", JSON.stringify(long));

  const shortBlocked = short.verdict?.allowed === false && short.outcome.ok === false;
  const longAllowed = long.verdict?.allowed === true;

  browser.ws.close();
  page.ws.close();

  if (shortBlocked && longAllowed) {
    console.log("\nPASS ✓  short order BLOCKED by policy; long order ALLOWED.");
    process.exit(0);
  }
  console.error("\nFAIL ✗  expected short blocked + long allowed.");
  process.exit(1);
}

main().catch((e) => {
  console.error("demo error:", e);
  process.exit(3);
});
