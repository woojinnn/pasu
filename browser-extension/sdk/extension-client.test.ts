import { describe, expect, it } from "vitest";
import { createExtensionClient } from "./extension-client";

// Smoke test for the eight Phase-6 SDK methods. happy-dom's
// `MessageEvent.source` is null (not the window), so the SDK's
// `event.source === window` guard rejects events posted via
// `window.postMessage`. We can't reproduce the production bridge here;
// instead we hook the SDK's transport by monkey-patching
// `window.postMessage` to deliver synthetic responses directly to the
// SDK's listener via `window.dispatchEvent`.

const REQ_TAG = "dambi-dashboard";
const RES_TAG = "dambi-extension";

interface Mocked {
  type: string;
  [key: string]: unknown;
}

function bridge(handler: (payload: Mocked) => unknown): () => void {
  const original = window.postMessage.bind(window);
  window.postMessage = ((message: unknown) => {
    const data = message as { source?: string; id?: string; payload?: Mocked };
    if (data?.source !== REQ_TAG || typeof data.id !== "string") {
      original(message, "*");
      return;
    }
    const response = handler(data.payload!);
    const event = new MessageEvent("message", {
      data: { source: RES_TAG, id: data.id, response },
      source: window as unknown as Window,
      origin: location.origin,
    });
    // Override the read-only `source` getter so the SDK's
    // `event.source === window` guard sees `window`.
    Object.defineProperty(event, "source", { value: window });
    window.dispatchEvent(event);
  }) as typeof window.postMessage;
  return () => {
    window.postMessage = original;
  };
}

describe("ExtensionClient Phase 6 manifest methods", () => {
  it("previewManifest forwards action/manifest and unwraps the response", async () => {
    const client = createExtensionClient({ timeoutMs: 500 });
    const close = bridge((payload) => {
      expect(payload.type).toBe("manifest:preview");
      expect(payload.action).toBe("swap");
      return {
        ok: true,
        data: {
          customTypes: [],
          enrichedSchemaText: "ok",
          diff: { added: [], removed: [], changed: [] },
          schemaHash: "sha256:zz",
        },
      };
    });
    try {
      const result = await client.previewManifest("swap", {
        id: "x",
        schema_version: 1,
        requires: [],
      });
      expect(result.schemaHash).toBe("sha256:zz");
    } finally {
      close();
    }
  });

  it("putManifest surfaces enrichedSchemaHash", async () => {
    const client = createExtensionClient({ timeoutMs: 500 });
    const close = bridge((payload) => {
      expect(payload.type).toBe("manifest:put");
      return {
        ok: true,
        data: { enrichedSchemaHash: "sha256:abc", addedCustomFields: {} },
      };
    });
    try {
      const r = await client.putManifest("swap", {
        id: "x",
        schema_version: 1,
        requires: [],
      });
      expect(r.enrichedSchemaHash).toBe("sha256:abc");
    } finally {
      close();
    }
  });

  it("the remaining six methods dispatch the right message types", async () => {
    const client = createExtensionClient({ timeoutMs: 500 });
    const seen: string[] = [];
    const close = bridge((payload) => {
      seen.push(payload.type);
      switch (payload.type) {
        case "manifest:get":
          return { ok: true, data: { manifest: null } };
        case "manifest:get-enriched-schema":
          return {
            ok: true,
            data: {
              schema_text: "",
              schema_hash: "sha256:1",
              added_fields: [],
              customContexts: {},
              schemaHash: "sha256:1",
            },
          };
        case "manifest:ping":
          return {
            ok: true,
            data: { reachable: true, url: "http://localhost:8787" },
          };
        case "manifest:alias-table":
          return { ok: true, data: { entries: [] } };
        case "migration:list":
          return { ok: true, data: { ids: ["dashboard::a"] } };
        case "migration:rewrite":
          return {
            ok: true,
            data: { id: "dashboard::a", rewritten: "y", applied: true },
          };
        default:
          return {
            ok: false,
            error: { kind: "wrong", message: String(payload.type) },
          };
      }
    });
    try {
      await client.getManifest("swap");
      await client.getEnrichedSchema();
      await client.pingRpcEndpoint();
      await client.getAliasTable();
      await client.listMigrationPending();
      await client.rewritePolicyToCustom({
        id: "dashboard::a",
        text: "x",
        knownFields: [],
      });
      expect(seen).toEqual([
        "manifest:get",
        "manifest:get-enriched-schema",
        "manifest:ping",
        "manifest:alias-table",
        "migration:list",
        "migration:rewrite",
      ]);
    } finally {
      close();
    }
  });

  it("propagates errors thrown by the bridge", async () => {
    const client = createExtensionClient({ timeoutMs: 500 });
    const close = bridge(() => ({
      ok: false,
      error: { kind: "schema_failed", message: "bad" },
    }));
    try {
      await expect(
        client.putManifest("swap", {
          id: "x",
          schema_version: 1,
          requires: [],
        }),
      ).rejects.toMatchObject({ kind: "schema_failed" });
    } finally {
      close();
    }
  });
});
