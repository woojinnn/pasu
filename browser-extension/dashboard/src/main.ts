import { createExtensionClient } from "@scopeball/sdk";

const client = createExtensionClient();

// Expose for console-driven exploration during dev.
(window as unknown as { c: typeof client }).c = client;

const statusEl = document.getElementById("status");

function setStatus(text: string, kind: "ok" | "err" | "pending"): void {
  if (!statusEl) return;
  statusEl.textContent = text;
  statusEl.className = kind === "pending" ? "" : kind;
}

async function refreshStatus(): Promise<void> {
  try {
    const { version } = await client.ping();
    const catalog = await client.getCatalog();
    setStatus(
      `connected (sdk v${version}) — ${catalog.policies.length} policies known, ${catalog.enabled.length} enabled, ${catalog.applied.length} applied`,
      "ok",
    );
  } catch (err) {
    setStatus(
      `extension not reachable: ${(err as Error).message}`,
      "err",
    );
  }
}

void refreshStatus();
client.onChange((keys) => {
  console.log("[scopeball] extension storage changed:", keys);
  void refreshStatus();
});
