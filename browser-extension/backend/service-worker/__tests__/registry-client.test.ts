/**
 * Phase 2B — Registry client cases.
 *
 * Covers the failure-mapping surface that drives the JIT fetcher's
 * negative-cache reasons:
 *   - 200 OK + matched shape  → ByCallKeyOk
 *   - 404                     → RegistryError("not_found", 404)
 *   - AbortController timeout → RegistryError("timeout")
 *   - fetch reject (network)  → RegistryError("network")
 *   - malformed JSON          → RegistryError("malformed_response")
 *   - bad shape (matched=false) → RegistryError("malformed_response")
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  byCallKey,
  callKeyUrl,
  RegistryError,
  type CallMatchKey,
} from "../registry/client";

const KEY: CallMatchKey = {
  chain_id: 1,
  to: "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D",
  selector: "0x38ED1739", // mixed-case on purpose — client must lowercase
};

function validBundlePayload() {
  return {
    matched: true,
    bundle_id: "uniswap/v2/swapExactTokensForTokens@1.0.0",
    manifest_path: "manifests/uniswap/v2/swapExactTokensForTokens@1.0.0.json",
    bundle_sha256: "0x9d54198599e1ced436bfbb458bf36aae4b3a01ba5a8bd885ab20f07c5a3f02f0",
    bundle: { id: "uniswap/v2/swapExactTokensForTokens@1.0.0" },
  };
}

describe("callKeyUrl", () => {
  it("lowercases `to` and `selector` to match build-index.ts filenames", () => {
    const url = callKeyUrl("http://localhost:8000", KEY);
    expect(url).toBe(
      "http://localhost:8000/index/by-callkey/1__0x7a250d5630b4cf539739df2c5dacb4c659f2488d__0x38ed1739.json",
    );
  });

  it("strips a trailing slash from the base URL", () => {
    const url = callKeyUrl("http://localhost:8000/", KEY);
    expect(url).not.toContain("//index/");
  });
});

describe("byCallKey", () => {
  // Typed as the spec'd fetch signature so the client's `fetchImpl?:
  // typeof fetch` accepts the mock without an `as` cast.
  let fetchMock: ReturnType<typeof vi.fn<typeof fetch>>;

  beforeEach(() => {
    fetchMock = vi.fn<typeof fetch>();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns the parsed ByCallKeyOk on 200 OK", async () => {
    const payload = validBundlePayload();
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify(payload), { status: 200 }),
    );

    const result = await byCallKey(KEY, { fetchImpl: fetchMock });

    expect(result.matched).toBe(true);
    expect(result.bundle_sha256).toBe(payload.bundle_sha256);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [calledUrl] = fetchMock.mock.calls[0];
    expect(calledUrl).toContain(
      "/index/by-callkey/1__0x7a250d5630b4cf539739df2c5dacb4c659f2488d__0x38ed1739.json",
    );
  });

  it("throws RegistryError('not_found') on HTTP 404", async () => {
    fetchMock.mockResolvedValueOnce(new Response("not found", { status: 404 }));

    try {
      await byCallKey(KEY, { fetchImpl: fetchMock });
      expect.fail("expected throw");
    } catch (err) {
      expect(err).toBeInstanceOf(RegistryError);
      expect((err as RegistryError).code).toBe("not_found");
      expect((err as RegistryError).status).toBe(404);
    }
  });

  it("throws RegistryError('timeout') when the fetch is aborted", async () => {
    // Real fetch behaviour: when AbortController fires, fetch rejects with
    // a DOMException-like { name: "AbortError" }. We simulate that by
    // listening to the signal in the mock and rejecting accordingly.
    fetchMock.mockImplementationOnce((_input, init) => {
      return new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => {
          const err = new Error("aborted");
          err.name = "AbortError";
          reject(err);
        });
      });
    });

    const promise = byCallKey(KEY, { fetchImpl: fetchMock, timeoutMs: 5 });
    await expect(promise).rejects.toBeInstanceOf(RegistryError);
    await expect(promise).rejects.toMatchObject({ code: "timeout" });
  });

  it("throws RegistryError('network') when the fetch promise rejects", async () => {
    fetchMock.mockRejectedValueOnce(new TypeError("Failed to fetch"));

    try {
      await byCallKey(KEY, { fetchImpl: fetchMock });
      expect.fail("expected throw");
    } catch (err) {
      expect(err).toBeInstanceOf(RegistryError);
      expect((err as RegistryError).code).toBe("network");
    }
  });

  it("throws RegistryError('malformed_response') on invalid JSON", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response("{not json", { status: 200 }),
    );

    try {
      await byCallKey(KEY, { fetchImpl: fetchMock });
      expect.fail("expected throw");
    } catch (err) {
      expect(err).toBeInstanceOf(RegistryError);
      expect((err as RegistryError).code).toBe("malformed_response");
    }
  });

  it("throws RegistryError('malformed_response') when shape is wrong (matched=false)", async () => {
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ matched: false }), { status: 200 }),
    );

    try {
      await byCallKey(KEY, { fetchImpl: fetchMock });
      expect.fail("expected throw");
    } catch (err) {
      expect(err).toBeInstanceOf(RegistryError);
      expect((err as RegistryError).code).toBe("malformed_response");
    }
  });

  it("throws RegistryError('network') on a non-404 5xx", async () => {
    fetchMock.mockResolvedValueOnce(new Response("boom", { status: 503 }));

    try {
      await byCallKey(KEY, { fetchImpl: fetchMock });
      expect.fail("expected throw");
    } catch (err) {
      expect(err).toBeInstanceOf(RegistryError);
      expect((err as RegistryError).code).toBe("network");
      expect((err as RegistryError).status).toBe(503);
    }
  });
});
